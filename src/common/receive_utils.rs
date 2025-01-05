use aes_gcm::{aead::Aead, Nonce};
use byteorder::{ReadBytesExt, BE};
use core::str;
use serde::de::DeserializeOwned;
use std::{
    fs::File,
    io::{Cursor, Write},
    path::Path,
    time::Duration,
};
use tokio::{io::AsyncReadExt, time::timeout};

use crate::common::{
    messages::{EncryptedMessage, HEADER_NAME_LENGTH, MESSAGE_LENGTH_LENGTH},
    CommunicationAgent, QuickTransferError,
};

use crate::common::QuickTransferStream;
use crate::common::QuickTransferStreamOption;

use super::{map_tcp_error, messages::UnencryptedMessage};

impl QuickTransferStream {
    /// TODO
    async fn receive_tcp(&mut self, wait: bool) -> Result<Vec<u8>, QuickTransferError> {
        let mut message_length_buffer: [u8; 8] = [0_u8; MESSAGE_LENGTH_LENGTH];
        if wait {
            // Read first byte:
            self.stream
                .read_exact(&mut message_length_buffer[0..1])
                .await
                .map_err(|error| map_tcp_error(error, self.role))?;
        }

        let status = if wait {
            self.stream.read_exact(&mut message_length_buffer[1..])
        } else {
            self.stream.read_exact(&mut message_length_buffer)
        };

        match timeout(Duration::from_secs(self.timeout.into()), status).await {
            Err(_) => {
                return Err(QuickTransferError::MessageReceiveTimeout(self.role));
            }
            Ok(result) => {
                result.map_err(|error| map_tcp_error(error, self.role))?;
            }
        }

        let bytes_to_receive = ReadBytesExt::read_u64::<BE>(&mut Cursor::new(message_length_buffer.to_vec())).map_err(|_| QuickTransferError::SentInvalidData(self.role))?;
        let mut received_data: Vec<u8> = vec![0_u8; bytes_to_receive.try_into().unwrap()];

        let status = self.stream.read_exact(&mut received_data);
        match timeout(Duration::from_secs(self.timeout.into()), status).await {
            Err(_) => {
                return Err(QuickTransferError::MessageReceiveTimeout(self.role));
            }
            Ok(result) => {
                result.map_err(|error| map_tcp_error(error, self.role))?;
            }
        }
    
        match &mut self.option {
            QuickTransferStreamOption::Unencrypted => {
                let deserialized_message: UnencryptedMessage = bincode::deserialize(&received_data).map_err(|_| QuickTransferError::SentInvalidData(self.role))?;

                Ok(deserialized_message.content)
            }
            QuickTransferStreamOption::Encrypted { cipher, .. } => {
                let deserialized_message: EncryptedMessage = bincode::deserialize(&received_data).map_err(|_| QuickTransferError::SentInvalidData(self.role))?;
                let nonce_array = Nonce::from_slice(&deserialized_message.nonce);

                let plain_text = cipher.decrypt(nonce_array, deserialized_message.content.as_ref()).map_err(|_| QuickTransferError::Deciphering)?;

                Ok(plain_text)
            }
        }
    }
    /// TODO
    pub async fn receive_bare_message_header(&mut self, timeout: u16) -> Result<String, QuickTransferError> {
        let mut message_header_buffer: [u8; 8] = [0_u8; HEADER_NAME_LENGTH];

        let status = self.stream.read_exact(&mut message_header_buffer);
        match tokio::time::timeout(Duration::from_secs(timeout.into()), status).await {
            Err(_) => {
                return Err(QuickTransferError::MessageReceiveTimeout(self.role));
            }
            Ok(result) => {
                result.map_err(|error| map_tcp_error(error, self.role))?;
            }
        }

        let header_received =
            str::from_utf8(&message_header_buffer).map_err(|_| QuickTransferError::SentInvalidData(self.role))?;

        Ok(String::from(header_received))
    }
}

impl CommunicationAgent<'_> {
    /// Receives one message package.
    /// If wait == true, then timeout for the first byte is not set.
    pub async fn receive_tcp(&mut self, wait: bool) -> Result<Vec<u8>, QuickTransferError> {
        self.stream.receive_tcp(wait).await
    }

    /// Receives a string (reads exactly `string_length` bytes so as to receive it).
    pub fn read_string<'a>(&mut self, message: &'a [u8], string_length: u64) -> Result<(String, &'a [u8]), QuickTransferError> {
        let string_length: usize = string_length.try_into().unwrap();
        let string = str::from_utf8(&message[..string_length])
            .map_err(|_| QuickTransferError::SentInvalidData(self.role))?;

        Ok((String::from(string), &message[string_length..]))
    }

    /// Reads a message header string (takes 8 bytes).
    pub fn read_message_header<'a>(&mut self, message: &'a [u8]) -> Result<(String, &'a [u8]), QuickTransferError> {
        self.read_string(message, HEADER_NAME_LENGTH.try_into().unwrap())
    }
    
    /// Receives (waits) a message header string (takes 8 bytes).
    pub async fn receive_message_header(&mut self) -> Result<String, QuickTransferError> {
        let message = self.receive_tcp(true).await?;
        let (header, _) = self.read_message_header(&message)?;

        Ok(header)
    }

    /// Reads a message header and automatically ensures it is equal to `message_header`.
    pub fn read_message_header_check<'a>(&mut self, message: &'a [u8], message_header: &str) -> Result<&'a [u8], QuickTransferError> {
        let (received_header, message) = self.read_message_header(message)?;

        if received_header != message_header {
            Err(QuickTransferError::SentInvalidData(self.role))
        } else {
            Ok(message)
        }
    }

    /// Receives a big-endian representation of a number (message length/file size/string size etc.) (takes 8 bytes).
    pub fn read_u64<'a>(&mut self, message: &'a [u8]) -> Result<(u64, &'a [u8]), QuickTransferError> {
        let read_number = ReadBytesExt::read_u64::<BE>(&mut Cursor::new(message[..MESSAGE_LENGTH_LENGTH].to_vec()))
            .map_err(|_| QuickTransferError::SentInvalidData(self.role))?;

        Ok((read_number, &message[MESSAGE_LENGTH_LENGTH..]))
    }

    /// Reads a string (its length and itself).
    pub fn read_length_with_string<'a>(&mut self, message: &'a [u8]) -> Result<(String, &'a [u8]), QuickTransferError> {
        let (string_length, message) = self.read_u64(message)?;
        self.read_string(message, string_length)
    }

    /// Reads an answer.
    pub fn read_answer<'a, T: DeserializeOwned>(&mut self, message: &'a [u8]) -> Result<T, QuickTransferError> {
        let (_, message) = self.read_u64(message)?;
        let deserialized_answer: T = bincode::deserialize(message).unwrap();

        Ok(deserialized_answer)
    }

    /// Receives a file and saves it in blocks (reads `file_size` bytes).
    pub async fn receive_file(
        &mut self,
        mut file: File,
        file_size: u64,
        file_path: &Path,
        try_all: bool,
    ) -> Result<(), QuickTransferError> {
        let mut bytes_to_receive_left = file_size;

        let mut just_receive = false;
        while bytes_to_receive_left > 0 {
            let file_block = self.receive_tcp(false).await?;
            let received_bytes = file_block.len();

            if !just_receive {
                let file_write_result = file.write_all(&file_block);
                if file_write_result.is_err() {
                    if try_all {
                        just_receive = true;
                    } else {
                        return Err(QuickTransferError::WritingFile {
                            file_path: String::from(file_path.to_str().unwrap()),
                        });
                    }
                }
            }

            let received_bytes: u64 = received_bytes.try_into().unwrap();
            bytes_to_receive_left -= received_bytes;
        }

        Ok(())
    }
}
