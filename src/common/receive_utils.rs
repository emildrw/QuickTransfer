use byteorder::{ReadBytesExt, BE};
use core::str;
use std::cmp::min;
use std::fs::File;
use std::io::{Cursor, ErrorKind, Read, Write};
use std::path::Path;

use crate::common::messages::{
    MessageDirectoryContents, HEADER_NAME_LENGTH, MESSAGE_LENGTH_LENGTH,
};

use crate::common::{CommunicationAgent, QuickTransferError};

use super::messages::{CdAnswer, FileFail, UploadResult, MAX_FILE_FRAGMENT_SIZE};

impl CommunicationAgent<'_> {
    /// Receives exactly this number of bytes to fill the buffer from TCP.
    fn receive_tcp(&mut self, message_buffer: &mut [u8]) -> Result<(), QuickTransferError> {
        self.stream.read_exact(message_buffer).map_err(|err| {
            if let ErrorKind::UnexpectedEof = err.kind() {
                return QuickTransferError::RemoteClosedConnection(self.role);
            }

            QuickTransferError::MessageReceive(self.role)
        })?;

        Ok(())
    }

    /// Receives a message header (takes 8 bytes).
    pub fn receive_message_header(&mut self) -> Result<String, QuickTransferError> {
        let mut buffer = [0_u8; HEADER_NAME_LENGTH];

        self.receive_tcp(&mut buffer)?;
        let header_received =
            str::from_utf8(&buffer).map_err(|_| QuickTransferError::SentInvalidData(self.role))?;

        Ok(String::from(header_received))
    }

    /// Receives a message header and automatically ensures it is equal to `message_header`.
    pub fn receive_message_header_check(
        &mut self,
        message_header: &str,
    ) -> Result<(), QuickTransferError> {
        let received = self.receive_message_header()?;

        if received != message_header {
            Err(QuickTransferError::SentInvalidData(self.role))
        } else {
            Ok(())
        }
    }

    /// Receives representing length of a message (string length, answer length, file size) (takes 8 bytes).
    pub fn receive_message_length(&mut self) -> Result<u64, QuickTransferError> {
        let mut buffer = [0_u8; MESSAGE_LENGTH_LENGTH];

        self.receive_tcp(&mut buffer)?;

        let read_number = Cursor::new(buffer.to_vec())
            .read_u64::<BE>()
            .map_err(|_| QuickTransferError::SentInvalidData(self.role))?;

        Ok(read_number)
    }

    /// Receives directory description (its size and itself).
    pub fn receive_directory_description(
        &mut self,
    ) -> Result<MessageDirectoryContents, QuickTransferError> {
        let description_length: usize = self.receive_message_length()?.try_into().unwrap();
        let mut buffer: Vec<u8> = vec![0_u8; description_length];
        self.receive_tcp(buffer.as_mut_slice())?;
        let deserialized_message: MessageDirectoryContents =
            bincode::deserialize(&buffer[..]).unwrap();

        Ok(deserialized_message)
    }

    /// Receives a string (reads exactly `string_length` bytes so as to receive it).
    pub fn receive_string(&mut self, string_length: u64) -> Result<String, QuickTransferError> {
        let string_length: usize = string_length.try_into().unwrap();
        let mut buffer: Vec<u8> = vec![0_u8; string_length];
        self.receive_tcp(buffer.as_mut_slice())?;
        let string = String::from_utf8(buffer)
            .map_err(|_| QuickTransferError::SentInvalidData(self.role))?;

        Ok(string)
    }

    /// Receives a CD message (only message length and message).
    pub fn receive_cd_message(&mut self) -> Result<String, QuickTransferError> {
        let dir_name_length = self.receive_message_length()?;
        let dir_name = self.receive_string(dir_name_length)?;

        Ok(dir_name)
    }

    /// Receives a CD answer message (only message length and message)
    pub fn receive_cd_answer(&mut self) -> Result<CdAnswer, QuickTransferError> {
        let answer_length: usize = self.receive_message_length()?.try_into().unwrap();
        let mut buffer: Vec<u8> = vec![0_u8; answer_length];
        self.receive_tcp(buffer.as_mut_slice())?;
        let deserialized_message: CdAnswer = bincode::deserialize(&buffer[..]).unwrap();

        Ok(deserialized_message)
    }

    /// Receives a string (its length and itself).
    pub fn receive_length_with_string(&mut self) -> Result<String, QuickTransferError> {
        let file_name_length = self.receive_message_length()?;
        let file_name = self.receive_string(file_name_length)?;

        Ok(file_name)
    }

    /// Receives a download fail message (only its length and message).
    pub fn receive_download_fail(&mut self) -> Result<FileFail, QuickTransferError> {
        let answer_length: usize = self.receive_message_length()?.try_into().unwrap();
        let mut buffer: Vec<u8> = vec![0_u8; answer_length];
        self.receive_tcp(buffer.as_mut_slice())?;
        let deserialized_message: FileFail = bincode::deserialize(&buffer[..]).unwrap();

        Ok(deserialized_message)
    }

    /// Receives a file and saves it in blocks (reads `file_size` bytes).
    pub fn receive_file(
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

            self.receive_tcp(&mut buffer[..now_receive_bytes])?;
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
    pub fn receive_upload_result(&mut self) -> Result<UploadResult, QuickTransferError> {
        let answer_length: usize = self.receive_message_length()?.try_into().unwrap();
        let mut buffer: Vec<u8> = vec![0_u8; answer_length];
        self.receive_tcp(buffer.as_mut_slice())?;
        let deserialized_message: UploadResult = bincode::deserialize(&buffer[..]).unwrap();

        Ok(deserialized_message)
    }
}
