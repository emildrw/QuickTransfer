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
pub const MESSAGE_MKDIR: &str = "MKDIR___";
pub const MESSAGE_MKDIRANS: &str = "MKDIRANS";
pub const MESSAGE_RENAME: &str = "RENAME__";
pub const MESSAGE_RENAME_ANSWER: &str = "RENAMEAN";
pub const MESSAGE_DISCONNECT: &str = "DISCONN_";

// Constants:
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
pub struct DirectoryContents {
    pub location: String,
    pub positions: Vec<DirectoryPosition>,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum MessageDirectoryContents {
    ReadingDirectoryError,
    Success(DirectoryContents),
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum CdAnswer {
    DirectoryDoesNotExist,
    IllegalDirectory,
    ReadingDirectoryError,
    Success(MessageDirectoryContents),
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum FileFail {
    FileDoesNotExist,
    IllegalFile,
    ErrorOpeningFile,
    ErrorCreatingFile,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum UploadResult {
    Fail(FileFail),
    Success,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum MkdirAnswer {
    DirectoryAlreadyExists,
    ErrorCreatingDirectory,
    IllegalDirectory,
    Success,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum RenameAnswer {
    FileDirDoesNotExist,
    IllegalFileDir,
    ErrorRenaming,
    Success,
}
