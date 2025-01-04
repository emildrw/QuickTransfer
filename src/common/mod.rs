use core::fmt;
use aes::{cipher::ArrayLength, Aes256};
use aes_gcm::{aead::{AeadMut, OsRng}, Nonce};
use aes_gcm::{AesGcm, TagSize};
use byteorder::{ReadBytesExt, WriteBytesExt, BE};
use messages::{DirectoryContents, DirectoryPosition, MessageDirectoryContents, HEADER_NAME_LENGTH};
use std::{
    collections::VecDeque, fs::{self, DirEntry}, io::{self, Cursor, ErrorKind, Write}, path::Path, str
};
use thiserror::Error;
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream};
use rand::RngCore;

pub mod messages;
mod receive_utils;
mod send_utils;

// Generic constants:
pub const DEFAULT_PORT: u16 = 47842;
pub const DEFAULT_TIMEOUT: u16 = 5;

// Enums and structs:
#[derive(Copy, Clone, Debug)]
pub enum ProgramRole {
    Server,
    Client,
}

impl fmt::Display for ProgramRole {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            if let ProgramRole::Server = self {
                "server"
            } else {
                "client"
            }
        )
    }
}

pub struct ProgramOptions {
    pub program_role: ProgramRole,
    pub server_ip_address: String,
    pub port: u16,
    pub root_directory: String,
    pub timeout: u16,
    pub aes_key: Option<[u8; 32]>,
}

/// A helper providing an abstraction for sending and receiving messages.
pub struct CommunicationAgent<'a, NonceSize: ArrayLength<u8>, TS: TagSize> {
    stream: &'a mut QuickTransferStream<NonceSize, TS>,
    role: ProgramRole,
    timeout: u16,
}

impl<NonceSize: ArrayLength<u8>, TS: TagSize> CommunicationAgent<'_, NonceSize, TS> {
    pub fn new(stream: &mut QuickTransferStream<NonceSize, TS>, role: ProgramRole, timeout: u16) -> CommunicationAgent<NonceSize, TS> {
        CommunicationAgent {
            stream,
            role,
            timeout,
        }
    }
}

enum QuickTransferStreamOption {
    Unencrypted,
    Encrypted {
        buffer: VecDeque<u8>,
    },
}

pub struct QuickTransferStream<NonceSize: ArrayLength<u8>, TS: TagSize> {
    option: QuickTransferStreamOption,
    stream: TcpStream,
    cipher: AesGcm<Aes256, NonceSize, TS>,
    role: ProgramRole,
}

impl<NonceSize: ArrayLength<u8>, TS: TagSize> QuickTransferStream<NonceSize, TS> {
    pub fn new_unencrypted(stream: TcpStream, cipher: AesGcm<Aes256, NonceSize, TS>, role: ProgramRole) -> QuickTransferStream<NonceSize, TS> {
        QuickTransferStream {
            option: QuickTransferStreamOption::Unencrypted,
            stream,
            cipher,
            role
        }
    }
    pub fn new_encrypted(stream: TcpStream, cipher: AesGcm<Aes256, NonceSize, TS>, role: ProgramRole) -> QuickTransferStream<NonceSize, TS> {
        QuickTransferStream {
            option: QuickTransferStreamOption::Encrypted{
                buffer: VecDeque::new(),
            },
            cipher,
            stream,
            role: role,
        }
    }
    pub fn change_to_encrypted(&mut self) {
        self.option = QuickTransferStreamOption::Encrypted {
            buffer: VecDeque::new(),
        };
    }
    async fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        match &mut self.option {
            QuickTransferStreamOption::Unencrypted => self.stream.write_all(buf).await,
            QuickTransferStreamOption::Encrypted{..} => {
                let mut nonce = vec![0u8; 12];
                OsRng.fill_bytes(&mut nonce);
                let nonce_array = Nonce::from_slice(&nonce);
                let cipher_text = self.cipher.encrypt(nonce_array, buf).map_err(|_| io::Error::new(ErrorKind::InvalidData, ""))?;

                let mut to_send: Vec<u8> = vec![];
                let message = bincode::serialize(
                    &(nonce, cipher_text)
                ).map_err(|_| io::Error::new(ErrorKind::InvalidData, ""))?;

                // We assume that usize <= u64:
                WriteBytesExt::write_u64::<BE>(&mut to_send, message.len().try_into().unwrap()).map_err(|_| io::Error::new(ErrorKind::InvalidData, ""))?;
                to_send.extend(message);

                self.stream.write_all(&to_send).await
            }
        }
    }
    async fn flush(&mut self) -> io::Result<()> {
        self.stream.flush().await
    }
    async fn read_exact(&mut self, mut buf: &mut [u8]) -> io::Result<usize> {
        match &mut self.option {
            QuickTransferStreamOption::Unencrypted => self.stream.read_exact(buf).await,
            QuickTransferStreamOption::Encrypted{buffer} => {
                while buffer.len() < buf.len() {
                    let mut buffer_msg = [0_u8; 8];
                    self.stream.read_exact(&mut buffer_msg).await?;
                    let to_receive = ReadBytesExt::read_u64::<BE>(&mut Cursor::new(buffer_msg.to_vec()))?;
    
                    let mut received_data: Vec<u8> = vec![0_u8; to_receive.try_into().unwrap()];
                    self.stream.read_exact(&mut received_data).await?;
                    let deserialized_message: (Vec<u8>, Vec<u8>) = bincode::deserialize(&received_data).map_err(|_| io::Error::new(ErrorKind::InvalidData, ""))?;
                    let nonce_array = Nonce::from_slice(&deserialized_message.0);
    
                    let plain_text = self.cipher.decrypt(nonce_array, deserialized_message.1.as_ref()).map_err(|_| io::Error::new(ErrorKind::InvalidData, ""))?;

                    buffer.extend(plain_text);
                }

                let message: Vec<u8> = buffer.drain(0..buf.len()).collect();
                buf.write_all(&message)?;

                Ok(buf.len())
            }
        }
    }
    pub async fn send_message_bare(&mut self, message: &str) -> Result<(), QuickTransferError> {
        let message = message.as_bytes();
        self.stream.write_all(message).await.map_err(|err| {
            if let ErrorKind::UnexpectedEof = err.kind() {
                QuickTransferError::RemoteClosedConnection(self.role)
            } else {
                QuickTransferError::ErrorWhileSendingMessage(self.role)
            }
        })?;

        self.stream.flush().await.map_err(|_: io::Error| {
            QuickTransferError::ErrorWhileSendingMessage(self.role)
        })?;

        Ok(())
    }
    pub async fn receive_message_header_bare(&mut self) -> Result<String, QuickTransferError> {
        let mut message_buffer: [u8; 8] = [0_u8; HEADER_NAME_LENGTH];
        self.stream.read_exact(&mut message_buffer).await.map_err(|err| {
            if let ErrorKind::UnexpectedEof = err.kind() {
                return QuickTransferError::RemoteClosedConnection(self.role);
            }

            QuickTransferError::MessageReceive(self.role)
        })?;

        let header_received =
            str::from_utf8(&message_buffer).map_err(|_| QuickTransferError::SentInvalidData(self.role))?;

        Ok(String::from(header_received))
    }
}

impl<NonceSize: ArrayLength<u8>, TS: TagSize> CommunicationAgent<'_, NonceSize, TS> {
    pub async fn send_message_bare(&mut self, message: &str) -> Result<(), QuickTransferError> {
        self.stream.send_message_bare(message).await
    }
    pub async fn receive_message_header_bare(&mut self) -> Result<String, QuickTransferError> {
        self.stream.receive_message_header_bare().await
    }
    pub fn change_to_encrypted(&mut self) {
        self.stream.change_to_encrypted();
    }
}

// Helper functions:

/// Receives name of the opposite side computer name.
fn read_opposite_role(role: &ProgramRole, capitalize: bool) -> &'static str {
    if let ProgramRole::Server = role {
        if capitalize {
            "Client"
        } else {
            "client"
        }
    } else if capitalize {
        "Server"
    } else {
        "server"
    }
}

/// Returns directory description that can be sent.
pub fn directory_description(
    directory_path: &Path,
    root_directory_path: &Path,
) -> Result<MessageDirectoryContents, QuickTransferError> {
    let root = root_directory_path.to_str().unwrap();
    let mut path_displayed =
        String::from(directory_path.to_str().unwrap().strip_prefix(root).unwrap());

    if root == "/" && !path_displayed.is_empty() {
        path_displayed.insert(0, '/');
    }
    path_displayed.insert(0, '.');

    let paths =
        fs::read_dir(directory_path).map_err(|_| QuickTransferError::ReadingDirectoryContents)?;
    let directory_contents: Vec<Result<DirEntry, std::io::Error>> = paths.collect();
    if directory_contents.iter().any(|dir| dir.is_err()) {
        return Err(QuickTransferError::ReadingDirectoryContents);
    }

    let mut error_loading_contents = false;

    let directory_path_name = path_displayed;
    let directory_contents = MessageDirectoryContents::Success(DirectoryContents {
        location: directory_path_name,
        positions: directory_contents
            .into_iter()
            .map(|dir| dir.unwrap().path())
            .map(|path: std::path::PathBuf| {
                let mut file_name = path
                    .to_str()
                    .unwrap_or_else(|| {
                        error_loading_contents = true;
                        "?"
                    })
                    .strip_prefix(directory_path.to_str().unwrap())
                    .unwrap_or_else(|| {
                        error_loading_contents = true;
                        "?"
                    });

                if file_name.starts_with("/") {
                    // For unix paths
                    file_name = file_name.strip_prefix("/").unwrap_or(file_name);
                } else if file_name.starts_with("\\") {
                    // For Windows paths
                    file_name = file_name.strip_prefix("\\").unwrap_or(file_name);
                }

                DirectoryPosition {
                    name: String::from(file_name.strip_prefix("./").unwrap_or(file_name)),
                    is_directory: path.is_dir(),
                }
            })
            .collect(),
    });

    if error_loading_contents {
        return Err(QuickTransferError::ReadingDirectoryContents);
    }

    Ok(directory_contents)
}

/// Custom error enum.
#[derive(Error, Debug)]
pub enum QuickTransferError {
    #[error("An error occurred while creating a server. Please try again.")]
    ServerCreation,

    #[error("Server \"{server_ip}\" refused the connection. Make sure this is a correct address and the server is running QuickTransfer on port {port}.")]
    ConnectionRefused { server_ip: String, port: u16 },

    #[error("An error occurred while connecting. Please try again.")]
    ConnectionCreation,

    #[error("An error occurred while receiving a message from {}.", read_opposite_role(.0, false))]
    MessageReceive(ProgramRole),

    #[error("Timeout for receiving the message from {} has passed.", read_opposite_role(.0, false))]
    MessageReceiveTimeout(ProgramRole),

    #[error("{} closed the connection. Turn on QuickTransfer on {} computer again.", read_opposite_role(.0, true), read_opposite_role(.0, false))]
    RemoteClosedConnection(ProgramRole),

    #[error("{} has sent invalid data. Please try again.", read_opposite_role(.0, true))]
    SentInvalidData(ProgramRole),

    #[error("An error occurred while sending message to the {}.", read_opposite_role(.0, false))]
    ErrorWhileSendingMessage(ProgramRole),

    #[error("An error occurred while reading current directory contents. Make sure the program has permissions to do so. It is needed for QuickTransfer to work.")]
    ReadingDirectoryContents,

    #[error("A fatal error has occurred.")]
    Fatal,

    #[error("A problem with writing to stdout has occurred.")]
    Stdout,

    #[error("A problem with reading user's input: {error}")]
    ReadLine { error: String },

    #[error("A problem with opening file `{file_path}` has occurred.")]
    OpeningFile { file_path: String },

    #[error("A problem with reading file `{file_path}` has occurred.")]
    ReadingFile { file_path: String },

    #[error("A problem with writing file `{file_path}` has occurred.")]
    WritingFile { file_path: String },

    #[error("Server doesn't support encryption.")]
    ServerDoesNotSupportEncryption,

    #[error("")]
    Other,
}
