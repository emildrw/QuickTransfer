use serde::{Deserialize, Serialize};

// Messages headers:
pub const HEADER_NAME_LENGTH: usize = 8;
pub const MESSAGE_INIT: &str = "INIT____";
pub const MESSAGE_INIT_OK: &str = "INIT_OK_";

// Messages bodies:
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct DirectoryPosition {
    pub name: String,
    pub is_directory: bool,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct MessageDirectoryContents(pub Vec<DirectoryPosition>);
