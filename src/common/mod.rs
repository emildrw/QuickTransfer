use aes::{cipher::typenum, Aes256};
use aes_gcm::AesGcm;
use core::fmt;
use messages::{DirectoryContents, DirectoryPosition, MessageDirectoryContents};
use std::{
    fs::{self, DirEntry},
    io::{self, ErrorKind},
    path::Path,
    str,
};
use thiserror::Error;
use tokio::net::TcpStream;

pub mod messages;
mod receive_utils;
mod send_utils;

// Generic constants:
pub const DEFAULT_PORT: u16 = 47842;
pub const DEFAULT_TIMEOUT: u16 = 5;

// Functions:
pub fn map_tcp_error(error: io::Error, role: ProgramRole) -> QuickTransferError {
    if let ErrorKind::UnexpectedEof = error.kind() {
        return QuickTransferError::RemoteClosedConnection(role);
    }

    QuickTransferError::MessageReceive(role)
}

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
pub struct CommunicationAgent<'a> {
    stream: &'a mut QuickTransferStream,
    role: ProgramRole,
    timeout: u16,
}

impl CommunicationAgent<'_> {
    pub fn new(
        stream: &mut QuickTransferStream,
        role: ProgramRole,
        timeout: u16,
    ) -> CommunicationAgent {
        CommunicationAgent {
            stream,
            role,
            timeout,
        }
    }
}

type CipherType = AesGcm<Aes256, typenum::U12, typenum::U16>;

enum QuickTransferStreamOption {
    Unencrypted,
    Encrypted { cipher: Box<CipherType> },
}

pub struct QuickTransferStream {
    option: QuickTransferStreamOption,
    stream: TcpStream,
    role: ProgramRole,
    timeout: u16,
}

impl QuickTransferStream {
    pub fn new_unencrypted(
        stream: TcpStream,
        role: ProgramRole,
        timeout: u16,
    ) -> QuickTransferStream {
        QuickTransferStream {
            option: QuickTransferStreamOption::Unencrypted,
            stream,
            role,
            timeout,
        }
    }
    pub fn new_encrypted(
        stream: TcpStream,
        cipher: CipherType,
        role: ProgramRole,
        timeout: u16,
    ) -> QuickTransferStream {
        QuickTransferStream {
            option: QuickTransferStreamOption::Encrypted {
                cipher: Box::new(cipher),
            },
            stream,
            role,
            timeout,
        }
    }
    pub fn change_to_encrypted(&mut self, cipher: CipherType) {
        self.option = QuickTransferStreamOption::Encrypted {
            cipher: Box::new(cipher),
        };
    }
}

impl CommunicationAgent<'_> {
    pub async fn send_bare_message(&mut self, message: &str) -> Result<(), QuickTransferError> {
        self.stream.send_bare_message(message).await
    }
    pub async fn receive_bare_message_header(&mut self) -> Result<String, QuickTransferError> {
        self.stream.receive_bare_message_header(self.timeout).await
    }
    pub fn change_to_encrypted(&mut self, cipher: CipherType) {
        self.stream.change_to_encrypted(cipher);
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

    #[error("An error occurred while deciphering. Make sure that client and server use the same AES256 key.")]
    Deciphering,

    #[error("An error occurred while ciphering.")]
    Ciphering,

    #[error("")]
    Other,
}

#[cfg(test)]
mod test {
    use aes::cipher::generic_array::GenericArray;
    use aes_gcm::{aead::Aead, Aes256Gcm, Key, KeyInit};
    use byteorder::{ReadBytesExt, WriteBytesExt, BE};
    use io::Cursor;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    use super::*;

    #[tokio::test]
    async fn test_send_bare_message() {
        let listener = TcpListener::bind("::1:9990").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0; 1024];
            let n = socket.read(&mut buf).await.unwrap();
            assert_eq!(&buf[..n], b"Hello, world!");
        });

        let stream = TcpStream::connect(addr).await.unwrap();
        let mut quick_transfer_stream =
            QuickTransferStream::new_unencrypted(stream, ProgramRole::Client, DEFAULT_TIMEOUT);
        let mut agent = CommunicationAgent::new(
            &mut quick_transfer_stream,
            ProgramRole::Client,
            DEFAULT_TIMEOUT,
        );

        agent.send_bare_message("Hello, world!").await.unwrap();
    }

    #[tokio::test]
    async fn test_receive_bare_message_header() {
        let listener = TcpListener::bind("::1:9991").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            socket.write_all(b"HEADERMS").await.unwrap();
        });

        let stream = TcpStream::connect(addr).await.unwrap();
        let mut quick_transfer_stream =
            QuickTransferStream::new_unencrypted(stream, ProgramRole::Client, DEFAULT_TIMEOUT);
        let mut agent = CommunicationAgent::new(
            &mut quick_transfer_stream,
            ProgramRole::Client,
            DEFAULT_TIMEOUT,
        );

        let header = agent.receive_bare_message_header().await.unwrap();
        assert_eq!(header, "HEADERMS");
    }

    #[tokio::test]
    async fn test_change_to_encrypted() {
        let listener = TcpListener::bind("::1:9992").await.unwrap();

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            socket.write_all(b"HEADERMS").await.unwrap();
        });

        let stream = TcpStream::connect("::1:9992").await.unwrap();
        let mut quick_transfer_stream =
            QuickTransferStream::new_unencrypted(stream, ProgramRole::Client, DEFAULT_TIMEOUT);
        let mut agent = CommunicationAgent::new(
            &mut quick_transfer_stream,
            ProgramRole::Client,
            DEFAULT_TIMEOUT,
        );

        let key = GenericArray::from_slice(&[0u8; 32]);
        let cipher = AesGcm::new(key);
        agent.change_to_encrypted(cipher);

        if let QuickTransferStreamOption::Encrypted { .. } = agent.stream.option {
            // Test passed
        } else {
            panic!("Stream was not changed to encrypted");
        }

        agent.receive_bare_message_header().await.unwrap();
    }

    #[tokio::test]
    async fn test_send_encrypted_message() {
        let listener = TcpListener::bind("::1:9993").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0; 1024];
            let n = socket.read(&mut buf).await.unwrap();
            // Assuming the message is encrypted, we can't assert the content directly
            assert!(n > 0);
        });

        let stream = TcpStream::connect(addr).await.unwrap();
        let key = GenericArray::from_slice(&[0u8; 32]);
        let cipher = Aes256Gcm::new(key);
        let mut quick_transfer_stream = QuickTransferStream::new_encrypted(
            stream,
            cipher,
            ProgramRole::Client,
            DEFAULT_TIMEOUT,
        );
        let mut agent = CommunicationAgent::new(
            &mut quick_transfer_stream,
            ProgramRole::Client,
            DEFAULT_TIMEOUT,
        );

        agent
            .send_bare_message("Hello, encrypted world!")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_receive_encrypted_message_header() {
        let listener = TcpListener::bind("::1:9994").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let nonce = GenericArray::from_slice(&[45_u8; 12]);
        let test_str = "I_NEED_TO_SOME_TEXT_OF_LENGTH_32";

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let key: &Key<Aes256Gcm> = GenericArray::from_slice(&[56_u8; 32]);
            let cipher: CipherType = Aes256Gcm::new(key);
            let cipher_text = cipher.encrypt(nonce, test_str.as_ref()).unwrap();
            println!("cipher: {:?}", cipher_text);

            let mut length_to_send: Vec<u8> = vec![];
            WriteBytesExt::write_u64::<BE>(
                &mut length_to_send,
                cipher_text.len().try_into().unwrap(),
            )
            .unwrap();

            socket.write_all(&length_to_send).await.unwrap();
            socket.write_all(&cipher_text).await.unwrap();

            eprintln!("OK_end");
        });

        let mut stream = TcpStream::connect(addr).await.unwrap();
        let key: &Key<Aes256Gcm> = GenericArray::from_slice(&[56_u8; 32]);
        let cipher: CipherType = Aes256Gcm::new(key);

        let mut buf_len = [0_u8; 8].to_vec();
        stream.read_exact(&mut buf_len).await.unwrap();
        let bytes_to_receive =
            ReadBytesExt::read_u64::<BE>(&mut Cursor::new(buf_len.to_vec())).unwrap();

        let mut buf: Vec<u8> = vec![0_u8; bytes_to_receive.try_into().unwrap()];
        stream.read_exact(&mut buf).await.unwrap();

        println!("received: {:?}", buf);
        let received_text = cipher.decrypt(nonce, buf.as_ref());

        assert_eq!(received_text.unwrap(), test_str.as_bytes());
    }
}
