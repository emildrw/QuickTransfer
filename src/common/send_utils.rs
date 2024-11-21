use byteorder::{WriteBytesExt, BE};
use std::fs::{self, DirEntry};
use std::io::Write;

use crate::common::{CommunicationAgent, ProgramRole, QuickTransferError};
use crate::messages::{DirectoryPosition, MessageDirectoryContents, MESSAGE_DIR, MESSAGE_INIT};

impl CommunicationAgent<'_> {
    pub fn send_tcp(&mut self, message: &[u8], flush: bool) -> Result<(), QuickTransferError> {
        let bytes_written = self.stream.write(message);
        if bytes_written.is_err() {
            return Err(QuickTransferError::new_from_string(format!(
                "An error occurred while sending message to {}.",
                if let ProgramRole::Client = self.role {
                    "server"
                } else {
                    "client"
                }
            )));
        } else if let Ok(n) = bytes_written {
            if n == 0 {
                return Err(QuickTransferError::new_from_string(format!(
                    "{} interrupted the connection. Turn on QuickTransfer on {} computer again.",
                    if let ProgramRole::Client = self.role {
                        "Server"
                    } else {
                        "Client"
                    },
                    if let ProgramRole::Client = self.role {
                        "server"
                    } else {
                        "client"
                    }
                )));
            }
        }

        if flush && self.stream.flush().is_err() {
            return Err(QuickTransferError::new_from_string(format!(
                "An error occurred while sending message to {}.",
                if let ProgramRole::Client = self.role {
                    "server"
                } else {
                    "client"
                }
            )));
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
        const READING_DIR_ERROR: &str = "An error occurred while reading current directory contents. Make sure the program has permission to do so. It is needed for QuickTransfer to work.";

        let paths = fs::read_dir(directory_path);
        if paths.is_err() {
            return Err(QuickTransferError::new(READING_DIR_ERROR));
        }
        let paths = paths.unwrap();

        let directory_contents: Vec<Result<DirEntry, std::io::Error>> = paths.collect();
        if directory_contents.iter().any(|dir| dir.is_err()) {
            return Err(QuickTransferError::new(READING_DIR_ERROR));
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

        dir_message
            .write_u64::<BE>(dir_description.len().try_into().unwrap())
            .unwrap();
        dir_message.extend(dir_description);

        // https://docs.rs/byteorder/1.5.0/byteorder/index.html

        if error_loading_contents {
            return Err(QuickTransferError::new(READING_DIR_ERROR));
        }

        self.send_tcp(dir_message.as_slice(), true)?;

        Ok(())
    }
}
