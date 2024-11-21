use core::fmt;
use std::net::TcpStream;
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
fn capitalize(string: String) -> String {
    let mut string_chars = string.chars();
    match string_chars.next() {
        None => string,
        Some(first_letter) => {
            first_letter.to_uppercase().collect::<String>() + string_chars.as_str()
        }
    }
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

    #[error("An error occurred while sending message to {0}.")]
    MessageReceive(ProgramRole),

    #[error("{} closed the connection. Turn on QuickTransfer on {} computer again.", capitalize(.0.to_string()), .0)]
    RemoteClosedConnection(ProgramRole),

    #[error("{} has sent invalid data. Please try again.", capitalize(.0.to_string()))]
    SentInvalidData(ProgramRole),

    #[error("An error occurred while sending message to the {0}.")]
    ErrorWhileSendingMessage(ProgramRole),

    #[error("An error occurred while reading current directory contents. Make sure the program has permission to do so. It is needed for QuickTransfer to work.")]
    ReadingDirectoryContents,

    #[error("A fatal error has occurred.")]
    FatalError,
}
