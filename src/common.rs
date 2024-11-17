use core::fmt;
use std::io::{Read, Write};
use std::net::TcpStream;

// Generic constants:
pub const DEFAULT_PORT: u16 = 47842;
pub const STREAM_BUFFER_SIZE: usize = 100;

// Messages headers:
pub const MESSAGE_INIT: &str = "INIT____";

pub enum ProgramRole {
    Server,
    Client,
}

pub struct ProgramOptions {
    pub program_role: ProgramRole,
    pub server_ip_address: String,
    pub port: u16,
}

pub fn send_tcp(
    stream: &mut TcpStream,
    message: &[u8],
    flush: bool,
    role: ProgramRole,
) -> Result<(), QuickTransferError> {
    let bytes_written = stream.write(message);
    if bytes_written.is_err() {
        return Err(QuickTransferError::new_from_string(format!(
            "An error occurred while sending message to {}.",
            if let ProgramRole::Client = role {
                "server"
            } else {
                "client"
            }
        )));
    } else if let Ok(n) = bytes_written {
        if n == 0 {
            return Err(QuickTransferError::new_from_string(format!(
                "{} interrupted the connection. Turn on QuickTransfer on {} computer again.",
                if let ProgramRole::Client = role {
                    "Server"
                } else {
                    "Client"
                },
                if let ProgramRole::Client = role {
                    "server"
                } else {
                    "client"
                }
            )));
        }
    }

    if flush && stream.flush().is_err() {
        return Err(QuickTransferError::new_from_string(format!(
            "An error occurred while sending message to {}.",
            if let ProgramRole::Client = role {
                "server"
            } else {
                "client"
            }
        )));
    }

    Ok(())
}

pub fn receive_tcp(
    stream: &mut TcpStream,
    message_buffer: &mut [u8],
    role: ProgramRole,
) -> Result<(), QuickTransferError> {
    let bytes_read = stream.read(message_buffer);
    if bytes_read.is_err() {
        return Err(QuickTransferError::new_from_string(format!(
            "An error occurred while receiving a message from {}.",
            if let ProgramRole::Client = role {
                "server"
            } else {
                "client"
            }
        )));
    } else if let Ok(n) = bytes_read {
        if n == 0 {
            return Err(QuickTransferError::new_from_string(format!(
                "{} interrupted the connection. Turn on QuickTransfer on {} computer again.",
                if let ProgramRole::Client = role {
                    "Server"
                } else {
                    "Client"
                },
                if let ProgramRole::Client = role {
                    "server"
                } else {
                    "client"
                }
            )));
        }
    }

    Ok(())
}

pub struct QuickTransferError {
    info: String,
}

impl QuickTransferError {
    pub fn new(info: &str) -> QuickTransferError {
        QuickTransferError {
            info: String::from(info),
        }
    }
    pub fn new_from_string(info: String) -> QuickTransferError {
        QuickTransferError { info }
    }
}

impl fmt::Display for QuickTransferError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.info)
    }
}
