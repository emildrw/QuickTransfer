use aes_gcm::{aead::Aead, Nonce};
use byteorder::{WriteBytesExt, BE};
use rand::{rngs::OsRng, RngCore};
use serde::Serialize;
use tokio::io::AsyncWriteExt;
use std::{
    fs::File,
    io::{self, Read},
    path::Path,
};

use crate::common::{
    directory_description,
    messages::{
        EncryptedMessage, UnencryptedMessage, MAX_FILE_FRAGMENT_SIZE, MESSAGE_CD, MESSAGE_DIR, MESSAGE_DISCONNECT, MESSAGE_DOWNLOAD, MESSAGE_DOWNLOAD_SUCCESS, MESSAGE_LS, MESSAGE_MKDIR, MESSAGE_UPLOAD
    },
    CommunicationAgent, QuickTransferError,
};

use super::{map_tcp_error, messages::{MessageDirectoryContents, MESSAGE_REMOVE, MESSAGE_RENAME}, QuickTransferStream};
use crate::common::QuickTransferStreamOption;

impl QuickTransferStream {
    async fn send_tcp(&mut self, message: &[u8], flush: bool) -> Result<(), QuickTransferError> {
        let message_to_send = match &mut self.option {
            QuickTransferStreamOption::Unencrypted => {
                let mut message_to_send: Vec<u8> = vec![];
                let message = bincode::serialize(&UnencryptedMessage{content: Vec::from(message)}).unwrap();

                // We assume that usize <= u64:
                WriteBytesExt::write_u64::<BE>(&mut message_to_send, message.len().try_into().unwrap()).unwrap();
                message_to_send.extend(message);

                message_to_send
            }
            QuickTransferStreamOption::Encrypted{cipher, ..} => {
                let mut nonce = vec![0u8; 12];
                OsRng.fill_bytes(&mut nonce);
                let nonce_array = Nonce::from_slice(&nonce);
                let cipher_text = cipher.encrypt(nonce_array, message).map_err(|_| QuickTransferError::Ciphering)?;

                let mut message_to_send: Vec<u8> = vec![];
                let message = bincode::serialize(&EncryptedMessage{nonce, content: cipher_text}).unwrap();

                // We assume that usize <= u64:
                WriteBytesExt::write_u64::<BE>(&mut message_to_send, message.len().try_into().unwrap()).unwrap();
                message_to_send.extend(message);

                message_to_send
            }
        };
        self.stream.write_all(&message_to_send).await.map_err(|error| map_tcp_error(error, self.role))?;
        if flush {
            self.stream.flush().await.map_err(|_| QuickTransferError::ErrorWhileSendingMessage(self.role))?;
        }

        Ok(())
    }
    pub async fn send_bare_message(&mut self, message: &str) -> Result<(), QuickTransferError> {
        let message = message.as_bytes();
        self.stream.write_all(message).await.map_err(|error| map_tcp_error(error, self.role))?;

        self.stream.flush().await.map_err(|_: io::Error| {
            QuickTransferError::ErrorWhileSendingMessage(self.role)
        })?;

        Ok(())
    }
}

impl CommunicationAgent<'_> {
    /// Send bytes from message over TCP.
    async fn send_tcp(&mut self, message: &[u8], flush: bool) -> Result<(), QuickTransferError> {
        self.stream.send_tcp(message, flush).await
    }

    /// Sends directory description: header, description length, description.
    pub async fn send_directory_description(
        &mut self,
        directory_path: &Path,
        root_directory_path: &Path,
    ) -> Result<(), QuickTransferError> {
        let directory_contents = directory_description(directory_path, root_directory_path);

        let mut dir_message = MESSAGE_DIR.as_bytes().to_vec();

        let dir_description = bincode::serialize(
            &directory_contents.unwrap_or(MessageDirectoryContents::ReadingDirectoryError),
        )
        .map_err(|_| QuickTransferError::ReadingDirectoryContents)?;

        // We assume that usize <= u64:
        WriteBytesExt::write_u64::<BE>(&mut dir_message, dir_description.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::Fatal)?;

        dir_message.extend(dir_description);

        self.send_tcp(dir_message.as_slice(), true).await?;

        Ok(())
    }

    /// Sends change directory message: header, directory name length, directory length.
    pub async fn send_change_directory(
        &mut self,
        directory_name: &str,
    ) -> Result<(), QuickTransferError> {
        let mut cd_message = MESSAGE_CD.as_bytes().to_vec();

        // We assume that usize <= u64:
        WriteBytesExt::write_u64::<BE>(&mut cd_message, directory_name.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::Fatal)?;

        cd_message.extend(directory_name.as_bytes());

        self.send_tcp(cd_message.as_slice(), true).await?;

        Ok(())
    }

    /// Sends a `ls` message (header).
    pub async fn send_list_directory(&mut self) -> Result<(), QuickTransferError> {
        self.send_tcp(MESSAGE_LS.as_bytes(), true).await?;

        Ok(())
    }

    /// Sends file download request: header, file name length, file name.
    pub async fn send_download_request(
        &mut self,
        file_name: &str,
    ) -> Result<(), QuickTransferError> {
        let mut download_message = MESSAGE_DOWNLOAD.as_bytes().to_vec();

        // We assume that usize <= u64:
        WriteBytesExt::write_u64::<BE>(&mut download_message, file_name.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::Fatal)?;

        download_message.extend(file_name.as_bytes());

        self.send_tcp(download_message.as_slice(), true).await?;

        Ok(())
    }

    /// Sends download success message: header, file size (in bytes) (without file contents!)
    pub async fn send_download_success(
        &mut self,
        file_size: u64,
    ) -> Result<(), QuickTransferError> {
        let mut download_success_message = MESSAGE_DOWNLOAD_SUCCESS.as_bytes().to_vec();

        // We assume that usize <= u64:
        WriteBytesExt::write_u64::<BE>(&mut download_success_message, file_size)
            .map_err(|_| QuickTransferError::Fatal)?;

        self.send_tcp(download_success_message.as_slice(), true)
            .await?;

        Ok(())
    }

    /// Sends a file (only bytes from that file) in blocks.
    pub async fn send_file(
        &mut self,
        mut file: File,
        file_size: u64,
        file_path: &Path,
    ) -> Result<(), QuickTransferError> {
        let mut bytes_to_send_left = file_size;
        let mut buffer = [0_u8; MAX_FILE_FRAGMENT_SIZE];
        while bytes_to_send_left > 0 {
            let read_bytes =
                file.read(&mut buffer)
                    .map_err(|_| QuickTransferError::ReadingFile {
                        file_path: String::from(file_path.to_str().unwrap()),
                    })?;

            if read_bytes == 0 {
                break;
            }
            let read_bytes_u64 = read_bytes.try_into().unwrap();

            self.send_tcp(&buffer[..read_bytes], bytes_to_send_left <= read_bytes_u64)
                .await?;
            bytes_to_send_left -= read_bytes_u64;
        }

        if bytes_to_send_left > 0 {
            return Err(QuickTransferError::ReadingFile {
                file_path: String::from(file_path.to_str().unwrap()),
            });
        }

        Ok(())
    }

    /// Sends an upload message: header, file size (in bytes), file contents.
    pub async fn send_upload(
        &mut self,
        file: File,
        file_size: u64,
        file_name: &str,
        file_path: &Path,
    ) -> Result<(), QuickTransferError> {
        let mut upload_message = MESSAGE_UPLOAD.as_bytes().to_vec();

        // We assume that usize <= u64:
        WriteBytesExt::write_u64::<BE>(&mut upload_message, file_name.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::Fatal)?;

        upload_message.extend(file_name.as_bytes());

        // We assume that usize <= u64:
        WriteBytesExt::write_u64::<BE>(&mut upload_message, file_size)
            .map_err(|_| QuickTransferError::Fatal)?;

        self.send_tcp(upload_message.as_slice(), true).await?;

        self.send_file(file, file_size, file_path).await?;

        Ok(())
    }

    /// Sends an mkdir message: header, name length, name.
    pub async fn send_mkdir(&mut self, directory_name: &str) -> Result<(), QuickTransferError> {
        let mut mkdir_message = MESSAGE_MKDIR.as_bytes().to_vec();

        // We assume that usize <= u64:
        WriteBytesExt::write_u64::<BE>(
            &mut mkdir_message,
            directory_name.len().try_into().unwrap(),
        )
        .map_err(|_| QuickTransferError::Fatal)?;

        mkdir_message.extend(directory_name.as_bytes());

        self.send_tcp(mkdir_message.as_slice(), true).await?;

        Ok(())
    }

    /// Sends rename request: header, file name length, file name.
    pub async fn send_rename_request(
        &mut self,
        file_name: &str,
        new_name: &str,
    ) -> Result<(), QuickTransferError> {
        let mut rename_message = MESSAGE_RENAME.as_bytes().to_vec();

        // We assume that usize <= u64:
        WriteBytesExt::write_u64::<BE>(&mut rename_message, file_name.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::Fatal)?;

        rename_message.extend(file_name.as_bytes());

        // We assume that usize <= u64:
        WriteBytesExt::write_u64::<BE>(&mut rename_message, new_name.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::Fatal)?;

        rename_message.extend(new_name.as_bytes());

        self.send_tcp(rename_message.as_slice(), true).await?;

        Ok(())
    }

    /// Sends remove request: header, file name length, file name.
    pub async fn send_remove_request(&mut self, file_name: &str) -> Result<(), QuickTransferError> {
        let mut remove_message = MESSAGE_REMOVE.as_bytes().to_vec();

        // We assume that usize <= u64:
        WriteBytesExt::write_u64::<BE>(&mut remove_message, file_name.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::Fatal)?;

        remove_message.extend(file_name.as_bytes());

        self.send_tcp(remove_message.as_slice(), true).await?;

        Ok(())
    }

    /// Send an answer.
    pub async fn send_answer<T: Serialize>(
        &mut self,
        massage_header: &str,
        answer: &T,
    ) -> Result<(), QuickTransferError> {
        let mut answer_message = massage_header.as_bytes().to_vec();
        let answer = bincode::serialize(answer).map_err(|_| QuickTransferError::Fatal)?;

        // We assume that usize <= u64:
        WriteBytesExt::write_u64::<BE>(&mut answer_message, answer.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::Fatal)?;

        answer_message.extend(answer);

        self.send_tcp(answer_message.as_slice(), true).await?;

        Ok(())
    }

    /// Sends a disconnect message (header).
    pub async fn send_disconnect_message(&mut self) -> Result<(), QuickTransferError> {
        self.send_tcp(MESSAGE_DISCONNECT.as_bytes(), true).await?;

        Ok(())
    }
}
