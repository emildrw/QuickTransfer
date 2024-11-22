use byteorder::{WriteBytesExt, BE};
use std::io::Write;
use std::path::Path;

use crate::common::messages::{MESSAGE_DIR, MESSAGE_INIT};
use crate::common::{CommunicationAgent, QuickTransferError};

use super::directory_description;
use super::messages::{CdAnswer, MESSAGE_CD, MESSAGE_CDANSWER};

impl CommunicationAgent<'_> {
    pub fn send_tcp(&mut self, message: &[u8], flush: bool) -> Result<(), QuickTransferError> {
        let bytes_written = self
            .stream
            .write(message)
            .map_err(|_| QuickTransferError::ErrorWhileSendingMessage(self.role))?;

        if bytes_written == 0 {
            return Err(QuickTransferError::RemoteClosedConnection(self.role));
        }

        if flush {
            self.stream
                .flush()
                .map_err(|_| QuickTransferError::ErrorWhileSendingMessage(self.role))?;
        }

        Ok(())
    }

    pub fn send_init_message(&mut self) -> Result<(), QuickTransferError> {
        self.send_tcp(MESSAGE_INIT.as_bytes(), true)?;

        Ok(())
    }

    pub fn send_directory_description(
        &mut self,
        directory_path: &Path,
    ) -> Result<(), QuickTransferError> {
        let directory_contents = directory_description(directory_path)?;

        let mut dir_message = MESSAGE_DIR.as_bytes().to_vec();

        let dir_description = bincode::serialize(&directory_contents)
            .map_err(|_| QuickTransferError::ReadingDirectoryContents)?;

        // We assume that usize <= u64:
        dir_message
            .write_u64::<BE>(dir_description.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::FatalError)?;

        dir_message.extend(dir_description);

        self.send_tcp(dir_message.as_slice(), true)?;

        Ok(())
    }

    pub fn send_change_directory(
        &mut self,
        directory_name: &str,
    ) -> Result<(), QuickTransferError> {
        let mut cd_message = MESSAGE_CD.as_bytes().to_vec();

        // We assume that usize <= u64:
        cd_message
            .write_u64::<BE>(directory_name.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::FatalError)?;

        cd_message.extend(directory_name.as_bytes());

        self.send_tcp(cd_message.as_slice(), true)?;

        Ok(())
    }

    pub fn send_cd_answer(&mut self, answer: &CdAnswer) -> Result<(), QuickTransferError> {
        let mut cdanswer_message = MESSAGE_CDANSWER.as_bytes().to_vec();

        let answer = bincode::serialize(answer).map_err(|_| QuickTransferError::FatalError)?;

        // We assume that usize <= u64:
        cdanswer_message
            .write_u64::<BE>(answer.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::FatalError)?;

        cdanswer_message.extend(answer);

        self.send_tcp(cdanswer_message.as_slice(), true)?;

        Ok(())
    }
}
