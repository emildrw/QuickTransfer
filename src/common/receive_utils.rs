use byteorder::{ReadBytesExt, BE};
use core::str;
use std::io::{Cursor, ErrorKind, Read};

use crate::common::messages::{
    MessageDirectoryContents, HEADER_NAME_LENGTH, MESSAGE_LENGTH_LENGTH,
};

use crate::common::{CommunicationAgent, QuickTransferError};

impl CommunicationAgent<'_> {
    pub fn receive_tcp(
        &mut self,
        message_buffer: &mut [u8]
    ) -> Result<(), QuickTransferError> {
        self.stream.read_exact(message_buffer).map_err(|err| {
            if let ErrorKind::UnexpectedEof = err.kind() {
                return QuickTransferError::RemoteClosedConnection(self.role);
            }
            return QuickTransferError::MessageReceive(self.role);
        })?;

        Ok(())
    }

    pub fn receive_message_header(
        &mut self,
        header: &'static str,
    ) -> Result<(), QuickTransferError> {
        let mut buffer = [0_u8; HEADER_NAME_LENGTH];

        self.receive_tcp(&mut buffer)?;
        let header_received =
            str::from_utf8(&buffer).map_err(|_| QuickTransferError::SentInvalidData(self.role))?;

        if header_received != header {
            return Err(QuickTransferError::SentInvalidData(self.role));
        }

        Ok(())
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
        description_length: u64,
    ) -> Result<MessageDirectoryContents, QuickTransferError> {
        let description_length: usize = description_length.try_into().unwrap();
        let mut buffer: Vec<u8> = vec![0_u8; description_length];
        self.receive_tcp(buffer.as_mut_slice())?;
        let deserialized_message: MessageDirectoryContents =
            bincode::deserialize(&buffer[..]).unwrap();

        Ok(deserialized_message)
    }
}
