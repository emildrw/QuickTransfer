use byteorder::{ReadBytesExt, BE};
use core::str;
use std::io::{Cursor, ErrorKind, Read};

use crate::common::messages::{
    MessageDirectoryContents, HEADER_NAME_LENGTH, MESSAGE_LENGTH_LENGTH,
};

use crate::common::{CommunicationAgent, QuickTransferError};

use super::messages::CdAnswer;

impl CommunicationAgent<'_> {
    fn receive_tcp(&mut self, message_buffer: &mut [u8]) -> Result<(), QuickTransferError> {
        self.stream.read_exact(message_buffer).map_err(|err| {
            if let ErrorKind::UnexpectedEof = err.kind() {
                return QuickTransferError::RemoteClosedConnection(self.role);
            }

            QuickTransferError::MessageReceive(self.role)
        })?;

        Ok(())
    }

    pub fn receive_message_header(&mut self) -> Result<String, QuickTransferError> {
        let mut buffer = [0_u8; HEADER_NAME_LENGTH];

        self.receive_tcp(&mut buffer)?;
        let header_received =
            str::from_utf8(&buffer).map_err(|_| QuickTransferError::SentInvalidData(self.role))?;

        Ok(String::from(header_received))
    }

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

    pub fn receive_message_length(&mut self) -> Result<u64, QuickTransferError> {
        let mut buffer = [0_u8; MESSAGE_LENGTH_LENGTH];

        self.receive_tcp(&mut buffer)?;

        let read_number = Cursor::new(buffer.to_vec())
            .read_u64::<BE>()
            .map_err(|_| QuickTransferError::SentInvalidData(self.role))?;

        Ok(read_number)
    }

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

    pub fn receive_string(&mut self, string_length: u64) -> Result<String, QuickTransferError> {
        let string_length: usize = string_length.try_into().unwrap();
        let mut buffer: Vec<u8> = vec![0_u8; string_length];
        self.receive_tcp(buffer.as_mut_slice())?;
        let string = String::from_utf8(buffer)
            .map_err(|_| QuickTransferError::SentInvalidData(self.role))?;

        Ok(string)
    }

    pub fn receive_cd_message(&mut self) -> Result<String, QuickTransferError> {
        let dir_name_length = self.receive_message_length()?;
        let dir_name = self.receive_string(dir_name_length)?;

        Ok(dir_name)
    }

    pub fn receive_cd_answer(&mut self) -> Result<CdAnswer, QuickTransferError> {
        let answer_length: usize = self.receive_message_length()?.try_into().unwrap();
        let mut buffer: Vec<u8> = vec![0_u8; answer_length];
        self.receive_tcp(buffer.as_mut_slice())?;
        let deserialized_message: CdAnswer = bincode::deserialize(&buffer[..]).unwrap();

        Ok(deserialized_message)
    }
}
