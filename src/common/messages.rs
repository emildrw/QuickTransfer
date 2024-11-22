use serde::{Deserialize, Serialize};

// Messages headers:
pub const HEADER_NAME_LENGTH: usize = 8;
pub const MESSAGE_LENGTH_LENGTH: usize = 8;
pub const MESSAGE_INIT: &str = "INIT____";
pub const MESSAGE_DIR: &str = "DIR_____";
pub const MESSAGE_CD: &str = "CD______";
pub const MESSAGE_CDANSWER: &str = "CDANSWER";

// Messages bodies:
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct DirectoryPosition {
    pub name: String,
    pub is_directory: bool,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct MessageDirectoryContents {
    location: String,
    positions: Vec<DirectoryPosition>,
}

impl MessageDirectoryContents {
    pub fn new(location: String, positions: Vec<DirectoryPosition>) -> MessageDirectoryContents {
        MessageDirectoryContents {
            location,
            positions,
        }
    }
    pub fn location(&self) -> &String {
        &self.location
    }
    pub fn positions(&self) -> &Vec<DirectoryPosition> {
        &self.positions
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum CdAnswer {
    DirectoryDoesNotExist,
    IllegalDirectory,
    Success(MessageDirectoryContents),
}
