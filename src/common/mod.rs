use core::fmt;
use messages::{DirectoryPosition, MessageDirectoryContents};
use std::{
    fs::{self, DirEntry},
    net::TcpStream,
    path::Path,
};
use thiserror::Error;

pub mod messages;
mod receive_utils;
mod send_utils;

// Generic constants:
pub const DEFAULT_PORT: u16 = 47842;

// Enums:
#[derive(Copy, Clone, Debug)]
pub enum ProgramRole {
    Server,
    Client,
}

impl fmt::Display for ProgramRole {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            if let ProgramRole::Server = self {
                "server"
            } else {
                "client"
            }
        )
    }
}

pub struct ProgramOptions {
    pub program_role: ProgramRole,
    pub server_ip_address: String,
    pub port: u16,
}

pub struct CommunicationAgent<'a> {
    stream: &'a mut TcpStream,
    role: ProgramRole,
}

impl CommunicationAgent<'_> {
    pub fn new(stream: &mut TcpStream, role: ProgramRole) -> CommunicationAgent {
        CommunicationAgent { stream, role }
    }
}

// Helper functions:
fn read_opposite_role(role: &ProgramRole, capitalize: bool) -> &'static str {
    if let ProgramRole::Server = role {
        if capitalize {
            "Client"
        } else {
            "client"
        }
    } else if capitalize {
        "Server"
    } else {
        "server"
    }
}

pub fn directory_description(
    directory_path: &Path,
) -> Result<MessageDirectoryContents, QuickTransferError> {
    let path = if directory_path.to_str().unwrap().is_empty() {
        Path::new("./")
    } else {
        directory_path
    };

    let paths = fs::read_dir(path).map_err(|_| QuickTransferError::ReadingDirectoryContents)?;
    let directory_contents: Vec<Result<DirEntry, std::io::Error>> = paths.collect();
    if directory_contents.iter().any(|dir| dir.is_err()) {
        return Err(QuickTransferError::ReadingDirectoryContents);
    }

    let mut error_loading_contents = false;

    let directory_path_name = String::from("./") + directory_path.to_str().unwrap();
    let directory_contents = MessageDirectoryContents::new(
        directory_path_name,
        directory_contents
            .into_iter()
            .map(|dir| dir.unwrap().path())
            .map(|path: std::path::PathBuf| {
                let file_name = path
                    .to_str()
                    .unwrap_or_else(|| {
                        error_loading_contents = true;
                        "?"
                    })
                    .strip_prefix(directory_path.to_str().unwrap())
                    .unwrap_or_else(|| {
                        error_loading_contents = true;
                        "?"
                    });

                DirectoryPosition {
                    name: String::from(file_name.strip_prefix("./").unwrap_or(file_name)),
                    is_directory: path.is_dir(),
                }
            })
            .collect(),
    );

    if error_loading_contents {
        return Err(QuickTransferError::ReadingDirectoryContents);
    }

    Ok(directory_contents)
}

// Custom error enum:
#[derive(Error, Debug)]
pub enum QuickTransferError {
    #[error("An error occurred while creating a server. Please try again.")]
    ServerCreation,

    #[error("Couldn't connect to server \"{server_ip}\". Make sure this is a correct address and the server is running QuickTransfer on port {port}.")]
    CouldntConnectToServer { server_ip: String, port: u16 },

    #[error("An error occurred while creating a connection. Please try again.")]
    ConnectionCreation,

    #[error("An error occurred while sending message to {}.", read_opposite_role(.0, false))]
    MessageReceive(ProgramRole),

    #[error("{} closed the connection. Turn on QuickTransfer on {} computer again.", read_opposite_role(.0, true), read_opposite_role(.0, false))]
    RemoteClosedConnection(ProgramRole),

    #[error("{} has sent invalid data. Please try again.", read_opposite_role(.0, true))]
    SentInvalidData(ProgramRole),

    #[error("An error occurred while sending message to the {}.", read_opposite_role(.0, false))]
    ErrorWhileSendingMessage(ProgramRole),

    #[error("An error occurred while reading current directory contents. Make sure the program has permission to do so. It is needed for QuickTransfer to work.")]
    ReadingDirectoryContents,

    #[error("A fatal error has occurred.")]
    FatalError,

    #[error("A problem with reading from stdin has occured.")]
    StdinError,

    #[error("A problem with writing on stdin has occured.")]
    StdoutError,

    #[error("A problem with opening file `{file_path}` has occured.")]
    ProblemOpeningFile { file_path: String },

    #[error("A problem with reading file `{file_path}` has occured.")]
    ProblemReadingFile { file_path: String },

    #[error("A problem with writing file `{file_path}` has occured.")]
    ProblemWritingFile { file_path: String },
}
