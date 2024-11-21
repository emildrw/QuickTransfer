use byteorder::{ReadBytesExt, BE};
use core::str;
use std::io::{Cursor, Read};

use crate::common::messages::{
    MessageDirectoryContents, HEADER_NAME_LENGTH, MESSAGE_LENGTH_LENGTH,
};

use crate::common::{CommunicationAgent, QuickTransferError};

impl CommunicationAgent<'_> {
    pub fn receive_tcp(
        &mut self,
        message_buffer: &mut [u8],
        bytes_no: usize,
    ) -> Result<(), QuickTransferError> {
        let mut read = |buffer: &mut [u8]| -> Result<usize, QuickTransferError> {
            let bytes_read = self
                .stream
                .read(buffer)
                .map_err(|_| QuickTransferError::MessageReceive(self.role))?;

            if bytes_read == 0 {
                return Err(QuickTransferError::RemoteClosedConnection(self.role));
            }

            Ok(bytes_read)
        };

        let mut bytes_read = 0_usize;
        while bytes_read < bytes_no {
            bytes_read += read(&mut message_buffer[bytes_read..bytes_no])?;
        }

        Ok(())
    }

    pub fn receive_message_header(
        &mut self,
        header: &'static str,
    ) -> Result<(), QuickTransferError> {
        let mut buffer = [0_u8; HEADER_NAME_LENGTH];

        self.receive_tcp(&mut buffer, HEADER_NAME_LENGTH)?;
        let header_received =
            str::from_utf8(&buffer).map_err(|_| QuickTransferError::SentInvalidData(self.role))?;

        if header_received != header {
            return Err(QuickTransferError::SentInvalidData(self.role));
        }

        Ok(())
    }

    pub fn receive_message_length(&mut self) -> Result<u64, QuickTransferError> {
        let mut buffer = [0_u8; MESSAGE_LENGTH_LENGTH];

        self.receive_tcp(&mut buffer, MESSAGE_LENGTH_LENGTH)?;

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
        self.receive_tcp(buffer.as_mut_slice(), description_length)?;
        let deserialized_message: MessageDirectoryContents =
            bincode::deserialize(&buffer[..]).unwrap();

        Ok(deserialized_message)
    }
}
