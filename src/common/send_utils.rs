use byteorder::{WriteBytesExt, BE};
use std::fs::{self, DirEntry};
use std::io::Write;

use crate::common::messages::{
    DirectoryPosition, MessageDirectoryContents, MESSAGE_DIR, MESSAGE_INIT,
};
use crate::common::{CommunicationAgent, QuickTransferError};

use super::messages::MESSAGE_CD;

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
        directory_path: &String,
    ) -> Result<(), QuickTransferError> {
        let paths = fs::read_dir(directory_path)
            .map_err(|_| QuickTransferError::ReadingDirectoryContents)?;

        let directory_contents: Vec<Result<DirEntry, std::io::Error>> = paths.collect();
        if directory_contents.iter().any(|dir| dir.is_err()) {
            return Err(QuickTransferError::ReadingDirectoryContents);
        }

        let mut error_loading_contents = false;

        let directory_contents = MessageDirectoryContents::new(
            String::from(directory_path),
            directory_contents
                .into_iter()
                .map(|dir| dir.unwrap().path())
                .map(|path: std::path::PathBuf| DirectoryPosition {
                    name: String::from(
                        path.to_str()
                            .unwrap_or_else(|| {
                                error_loading_contents = true;
                                "?"
                            })
                            .strip_prefix(directory_path)
                            .unwrap_or_else(|| {
                                error_loading_contents = true;
                                "?"
                            }),
                    ),
                    is_directory: path.is_dir(),
                })
                .collect(),
        );

        let mut dir_message = MESSAGE_DIR.as_bytes().to_vec();

        let dir_description = bincode::serialize(&directory_contents).unwrap_or_else(|_| {
            error_loading_contents = true;
            vec![]
        });

        // We assume that usize <= u64:
        dir_message
            .write_u64::<BE>(dir_description.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::FatalError)?;

        dir_message.extend(dir_description);

        if error_loading_contents {
            return Err(QuickTransferError::ReadingDirectoryContents);
        }

        self.send_tcp(dir_message.as_slice(), true)?;

        Ok(())
    }
    pub fn send_change_directory(
        &mut self,
        directory_name: &String,
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
}
