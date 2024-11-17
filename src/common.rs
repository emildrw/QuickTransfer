use core::{fmt, error, str};
use std::io::{Read, Write};
use std::net::TcpStream;

use crate::messages::HEADER_NAME_LENGTH;

// Generic constants:
pub const DEFAULT_PORT: u16 = 47842;
// pub const STREAM_BUFFER_SIZE: usize = 100;

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
    bytes_no: usize,
    role: ProgramRole,
) -> Result<(), QuickTransferError> {
    let mut read = |buffer: &mut [u8]| -> Result<usize, QuickTransferError> {
        let bytes_read = stream.read(buffer);
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

        Ok(bytes_read.unwrap())
    };

    let mut bytes_read = 0_usize;
    while bytes_read < bytes_no {
        bytes_read += read(&mut message_buffer[bytes_read..bytes_no])?;
    }

    Ok(())
}

pub fn receive_message_header(
    stream: &mut TcpStream,
    header: &'static str,
    role: ProgramRole,
) -> Result<(), QuickTransferError> {
    let mut buffer = [0_u8; HEADER_NAME_LENGTH];

    receive_tcp(stream, &mut buffer, HEADER_NAME_LENGTH, ProgramRole::Server)?;
    let header_received = str::from_utf8(&buffer);
    if header_received.is_err() {
        return Err(QuickTransferError::new("Client has sent invalid data. Please try again."));
    }
    let header_received = header_received.unwrap();
    if header_received != header {
        return Err(QuickTransferError::new_from_string(format!(
            "{} has sent invalid data. Please try again.",
            if let ProgramRole::Server = role {
                "Client"
            } else {
                "Server"
            }
        )));
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
