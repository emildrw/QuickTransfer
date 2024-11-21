use core::{fmt, str};
use std::net::TcpStream;

mod receive_utils;
mod send_utils;

// Generic constants:
pub const DEFAULT_PORT: u16 = 47842;

// Enums:
pub enum ProgramRole {
    Server,
    Client,
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

// Custom error enum:
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
