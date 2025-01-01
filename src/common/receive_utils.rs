use byteorder::{ReadBytesExt, BE};
use core::str;
use std::{
    cmp::min,
    fs::File,
    io::{Cursor, ErrorKind, Write},
    path::Path,
    time::Duration,
};
use tokio::{io::AsyncReadExt, time::timeout};

use crate::common::{
    messages::{
        CdAnswer, FileFail, MessageDirectoryContents, MkdirAnswer, UploadResult,
        HEADER_NAME_LENGTH, MAX_FILE_FRAGMENT_SIZE, MESSAGE_LENGTH_LENGTH,
    },
    CommunicationAgent, QuickTransferError,
};

use super::messages::RenameAnswer;

impl CommunicationAgent<'_> {
    /// Receives exactly this number of bytes to fill the buffer from TCP.
    async fn receive_tcp(&mut self, message_buffer: &mut [u8]) -> Result<(), QuickTransferError> {
        // Read first byte:
        self.stream
            .read_exact(&mut message_buffer[0..1])
            .await
            .map_err(|err| {
                if let ErrorKind::UnexpectedEof = err.kind() {
                    return QuickTransferError::RemoteClosedConnection(self.role);
                }

                QuickTransferError::MessageReceive(self.role)
            })?;

        let status = self.stream.read_exact(&mut message_buffer[1..]);
        match timeout(Duration::from_secs(self.timeout.into()), status).await {
            Err(_) => {
                return Err(QuickTransferError::MessageReceiveTimeout(self.role));
            }
            Ok(result) => {
                result.map_err(|err| {
                    if let ErrorKind::UnexpectedEof = err.kind() {
                        return QuickTransferError::RemoteClosedConnection(self.role);
                    }

                    QuickTransferError::MessageReceive(self.role)
                })?;
            }
        }

        Ok(())
    }

    /// Receives a message header (takes 8 bytes).
    pub async fn receive_message_header(&mut self) -> Result<String, QuickTransferError> {
        let mut buffer = [0_u8; HEADER_NAME_LENGTH];

        self.receive_tcp(&mut buffer).await?;
        let header_received =
            str::from_utf8(&buffer).map_err(|_| QuickTransferError::SentInvalidData(self.role))?;

        Ok(String::from(header_received))
    }

    /// Receives a message header and automatically ensures it is equal to `message_header`.
    pub async fn receive_message_header_check(
        &mut self,
        message_header: &str,
    ) -> Result<(), QuickTransferError> {
        let received = self.receive_message_header().await?;

        if received != message_header {
            Err(QuickTransferError::SentInvalidData(self.role))
        } else {
            Ok(())
        }
    }

    /// Receives representing length of a message (string length, answer length, file size) (takes 8 bytes).
    pub async fn receive_message_length(&mut self) -> Result<u64, QuickTransferError> {
        let mut buffer = [0_u8; MESSAGE_LENGTH_LENGTH];

        self.receive_tcp(&mut buffer).await?;

        let read_number = ReadBytesExt::read_u64::<BE>(&mut Cursor::new(buffer.to_vec()))
            .map_err(|_| QuickTransferError::SentInvalidData(self.role))?;

        Ok(read_number)
    }

    /// Receives directory description (its size and itself).
    pub async fn receive_directory_description(
        &mut self,
    ) -> Result<MessageDirectoryContents, QuickTransferError> {
        let description_length: usize = self.receive_message_length().await?.try_into().unwrap();
        let mut buffer: Vec<u8> = vec![0_u8; description_length];
        self.receive_tcp(buffer.as_mut_slice()).await?;
        let deserialized_message: MessageDirectoryContents =
            bincode::deserialize(&buffer[..]).unwrap();

        Ok(deserialized_message)
    }

    /// Receives a string (reads exactly `string_length` bytes so as to receive it).
    pub async fn receive_string(
        &mut self,
        string_length: u64,
    ) -> Result<String, QuickTransferError> {
        let string_length: usize = string_length.try_into().unwrap();
        let mut buffer: Vec<u8> = vec![0_u8; string_length];
        self.receive_tcp(buffer.as_mut_slice()).await?;
        let string = String::from_utf8(buffer)
            .map_err(|_| QuickTransferError::SentInvalidData(self.role))?;

        Ok(string)
    }

    /// Receives a CD message (only message length and message).
    pub async fn receive_cd_message(&mut self) -> Result<String, QuickTransferError> {
        let dir_name_length = self.receive_message_length().await?;
        let dir_name = self.receive_string(dir_name_length).await?;

        Ok(dir_name)
    }

    /// Receives a CD answer message (only message length and message)
    pub async fn receive_cd_answer(&mut self) -> Result<CdAnswer, QuickTransferError> {
        let answer_length: usize = self.receive_message_length().await?.try_into().unwrap();
        let mut buffer: Vec<u8> = vec![0_u8; answer_length];
        self.receive_tcp(buffer.as_mut_slice()).await?;
        let deserialized_message: CdAnswer = bincode::deserialize(&buffer[..]).unwrap();

        Ok(deserialized_message)
    }

    /// Receives a string (its length and itself).
    pub async fn receive_length_with_string(&mut self) -> Result<String, QuickTransferError> {
        let file_name_length = self.receive_message_length().await?;
        let file_name = self.receive_string(file_name_length).await?;

        Ok(file_name)
    }

    /// Receives a download fail message (only its length and message).
    pub async fn receive_download_fail(&mut self) -> Result<FileFail, QuickTransferError> {
        let answer_length: usize = self.receive_message_length().await?.try_into().unwrap();
        let mut buffer: Vec<u8> = vec![0_u8; answer_length];
        self.receive_tcp(buffer.as_mut_slice()).await?;
        let deserialized_message: FileFail = bincode::deserialize(&buffer[..]).unwrap();

        Ok(deserialized_message)
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
        let mut buffer = [0_u8; MAX_FILE_FRAGMENT_SIZE];

        let mut just_receive = false;
        while bytes_to_receive_left > 0 {
            let now_receive_bytes_u64: u64 = min(
                MAX_FILE_FRAGMENT_SIZE.try_into().unwrap(),
                bytes_to_receive_left,
            );
            let now_receive_bytes: usize = now_receive_bytes_u64.try_into().unwrap();

            self.receive_tcp(&mut buffer[..now_receive_bytes]).await?;
            if !just_receive {
                let file_write_result = file.write_all(&buffer[..now_receive_bytes]);
                if file_write_result.is_err() {
                    if try_all {
                        just_receive = true;
                    } else {
                        return Err(QuickTransferError::ProblemWritingFile {
                            file_path: String::from(file_path.to_str().unwrap()),
                        });
                    }
                }
            }

            bytes_to_receive_left -= now_receive_bytes_u64;
        }

        Ok(())
    }

    /// Receives upload result (only its length and message).
    pub async fn receive_upload_result(&mut self) -> Result<UploadResult, QuickTransferError> {
        let answer_length: usize = self.receive_message_length().await?.try_into().unwrap();
        let mut buffer: Vec<u8> = vec![0_u8; answer_length];
        self.receive_tcp(buffer.as_mut_slice()).await?;
        let deserialized_message: UploadResult = bincode::deserialize(&buffer[..]).unwrap();

        Ok(deserialized_message)
    }

    /// Receives a mkdir answer message (only message length and message)
    pub async fn receive_mkdir_answer(&mut self) -> Result<MkdirAnswer, QuickTransferError> {
        let answer_length: usize = self.receive_message_length().await?.try_into().unwrap();
        let mut buffer: Vec<u8> = vec![0_u8; answer_length];
        self.receive_tcp(buffer.as_mut_slice()).await?;
        let deserialized_message: MkdirAnswer = bincode::deserialize(&buffer[..]).unwrap();

        Ok(deserialized_message)
    }

    /// Receives a rename answer message (only message length and message)
    pub async fn receive_rename_answer(&mut self) -> Result<RenameAnswer, QuickTransferError> {
        let answer_length: usize = self.receive_message_length().await?.try_into().unwrap();
        let mut buffer: Vec<u8> = vec![0_u8; answer_length];
        self.receive_tcp(buffer.as_mut_slice()).await?;
        let deserialized_message: RenameAnswer = bincode::deserialize(&buffer[..]).unwrap();

        Ok(deserialized_message)
    }
}
