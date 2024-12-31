use core::fmt;
use messages::{DirectoryPosition, MessageDirectoryContents};
use std::{
    fs::{self, DirEntry},
    path::Path,
};
use thiserror::Error;
use tokio::net::TcpStream;

pub mod messages;
mod receive_utils;
mod send_utils;

// Generic constants:
pub const DEFAULT_PORT: u16 = 47842;

// Enums and structs:
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
    pub root_directory: String,
}

/// A helper providing an abstraction for sending and receiving messages.
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

/// Receives name of the opposite side computer name.
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

/// Returns directory description that can be sent.
pub fn directory_description(
    directory_path: &Path,
    root_directory_path: &Path,
) -> Result<MessageDirectoryContents, QuickTransferError> {
    let root = root_directory_path.to_str().unwrap();
    let mut path_displayed =
        String::from(directory_path.to_str().unwrap().strip_prefix(root).unwrap());

    if root == "/" && !path_displayed.is_empty() {
        path_displayed.insert(0, '/');
    }
    path_displayed.insert(0, '.');

    let paths =
        fs::read_dir(directory_path).map_err(|_| QuickTransferError::ReadingDirectoryContents)?;
    let directory_contents: Vec<Result<DirEntry, std::io::Error>> = paths.collect();
    if directory_contents.iter().any(|dir| dir.is_err()) {
        return Err(QuickTransferError::ReadingDirectoryContents);
    }

    let mut error_loading_contents = false;

    let directory_path_name = path_displayed;
    let directory_contents = MessageDirectoryContents::new(
        directory_path_name,
        directory_contents
            .into_iter()
            .map(|dir| dir.unwrap().path())
            .map(|path: std::path::PathBuf| {
                let mut file_name = path
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

                if file_name.starts_with("/") {
                    // For unix paths
                    file_name = file_name.strip_prefix("/").unwrap_or(file_name);
                } else if file_name.starts_with("\\") {
                    // For Windows paths
                    file_name = file_name.strip_prefix("\\").unwrap_or(file_name);
                }

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

/// Custom error enum.
#[derive(Error, Debug)]
pub enum QuickTransferError {
    #[error("An error occurred while creating a server. Please try again.")]
    ServerCreation,

    #[error("Couldn't connect to server \"{server_ip}\". Make sure this is a correct address and the server is running QuickTransfer on port {port}.")]
    CouldntConnectToServer { server_ip: String, port: u16 },

    #[error("An error occurred while creating a connection. Please try again.")]
    ConnectionCreation,

    #[error("An error occurred while receiving a message from {}.", read_opposite_role(.0, false))]
    MessageReceive(ProgramRole),

    #[error("{} closed the connection. Turn on QuickTransfer on {} computer again.", read_opposite_role(.0, true), read_opposite_role(.0, false))]
    RemoteClosedConnection(ProgramRole),

    #[error("{} has sent invalid data. Please try again.", read_opposite_role(.0, true))]
    SentInvalidData(ProgramRole),

    #[error("An error occurred while sending message to the {}.", read_opposite_role(.0, false))]
    ErrorWhileSendingMessage(ProgramRole),

    #[error("An error occurred while reading current directory contents. Make sure the program has permissions to do so. It is needed for QuickTransfer to work.")]
    ReadingDirectoryContents,

    #[error("A fatal error has occurred.")]
    FatalError,

    #[error("A problem with writing to stdout has occurred.")]
    StdoutError,

    #[error("A problem with reading user's input: {error}")]
    ReadLineError { error: String },

    #[error("A problem with opening file `{file_path}` has occurred.")]
    ProblemOpeningFile { file_path: String },

    #[error("A problem with reading file `{file_path}` has occurred.")]
    ProblemReadingFile { file_path: String },

    #[error("A problem with writing file `{file_path}` has occurred.")]
    ProblemWritingFile { file_path: String },
}
