use serde::{Deserialize, Serialize};

// Messages headers:
pub const MESSAGE_INIT: &str = "INIT____";
pub const MESSAGE_DIR: &str = "DIR_____";
pub const MESSAGE_CD: &str = "CD______";
pub const MESSAGE_CDANSWER: &str = "CDANSWER";
pub const MESSAGE_LS: &str = "LS______";
pub const MESSAGE_DOWNLOAD: &str = "DOWNLOAD";
pub const MESSAGE_DOWNLOAD_FAIL: &str = "DOWNFAIL";
pub const MESSAGE_DOWNLOAD_SUCCESS: &str = "DOWNSUCC";
pub const MESSAGE_UPLOAD: &str = "UPLOAD__";
pub const MESSAGE_UPLOAD_RESULT: &str = "UPLOADRE";
pub const MESSAGE_DISCONNECT: &str = "DISCONN_";

// Constans:
pub const HEADER_NAME_LENGTH: usize = 8;
pub const MESSAGE_LENGTH_LENGTH: usize = 8;
pub const MAX_FILE_FRAGMENT_SIZE: usize = 1024;
pub const ECONNREFUSED: i32 = 111;

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

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum FileFail {
    FileDoesNotExist,
    IllegalFile,
    ErrorCreatingFile,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum UploadResult {
    Fail(FileFail),
    Success,
}
