use byteorder::{WriteBytesExt, BE};
use std::fs::File;
use std::io::{ErrorKind, Read, Write};
use std::path::Path;

use crate::common::messages::{MESSAGE_DIR, MESSAGE_INIT, MESSAGE_UPLOAD};
use crate::common::{CommunicationAgent, QuickTransferError};

use super::directory_description;
use super::messages::{
    CdAnswer, FileFail, UploadResult, MAX_FILE_FRAGMENT_SIZE, MESSAGE_CD, MESSAGE_CDANSWER, MESSAGE_DISCONNECT, MESSAGE_DOWNLOAD, MESSAGE_DOWNLOAD_FAIL, MESSAGE_DOWNLOAD_SUCCESS, MESSAGE_LS, MESSAGE_UPLOAD_RESULT
};

impl CommunicationAgent<'_> {
    fn send_tcp(&mut self, message: &[u8], flush: bool) -> Result<(), QuickTransferError> {
        self.stream.write_all(message).map_err(|err| {
            if let ErrorKind::UnexpectedEof = err.kind() {
                QuickTransferError::RemoteClosedConnection(self.role)
            } else {
                QuickTransferError::ErrorWhileSendingMessage(self.role)
            }
        })?;

        if flush {
            self.stream
                .flush()
                .map_err(|_| QuickTransferError::ErrorWhileSendingMessage(self.role))?;
        }

        Ok(())
    }

    pub fn send_init_message(&mut self) -> Result<(), QuickTransferError> {
        self.send_tcp(MESSAGE_INIT.as_bytes(), true)?;

        Ok(())
    }

    pub fn send_directory_description(
        &mut self,
        directory_path: &Path,
        root_directory_path: &Path,
    ) -> Result<(), QuickTransferError> {
        let directory_contents = directory_description(directory_path, root_directory_path)?;

        let mut dir_message = MESSAGE_DIR.as_bytes().to_vec();

        let dir_description = bincode::serialize(&directory_contents)
            .map_err(|_| QuickTransferError::ReadingDirectoryContents)?;

        // We assume that usize <= u64:
        dir_message
            .write_u64::<BE>(dir_description.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::FatalError)?;

        dir_message.extend(dir_description);

        self.send_tcp(dir_message.as_slice(), true)?;

        Ok(())
    }

    pub fn send_change_directory(
        &mut self,
        directory_name: &str,
    ) -> Result<(), QuickTransferError> {
        let mut cd_message = MESSAGE_CD.as_bytes().to_vec();

        // We assume that usize <= u64:
        cd_message
            .write_u64::<BE>(directory_name.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::FatalError)?;

        cd_message.extend(directory_name.as_bytes());

        self.send_tcp(cd_message.as_slice(), true)?;

        Ok(())
    }

    pub fn send_cd_answer(&mut self, answer: &CdAnswer) -> Result<(), QuickTransferError> {
        let mut cdanswer_message = MESSAGE_CDANSWER.as_bytes().to_vec();
        let answer = bincode::serialize(answer).map_err(|_| QuickTransferError::FatalError)?;

        // We assume that usize <= u64:
        cdanswer_message
            .write_u64::<BE>(answer.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::FatalError)?;

        cdanswer_message.extend(answer);

        self.send_tcp(cdanswer_message.as_slice(), true)?;

        Ok(())
    }

    pub fn send_list_directory(&mut self) -> Result<(), QuickTransferError> {
        self.send_tcp(MESSAGE_LS.as_bytes(), true)?;

        Ok(())
    }

    pub fn send_download_request(&mut self, file_name: &str) -> Result<(), QuickTransferError> {
        let mut download_message = MESSAGE_DOWNLOAD.as_bytes().to_vec();

        // We assume that usize <= u64:
        download_message
            .write_u64::<BE>(file_name.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::FatalError)?;

        download_message.extend(file_name.as_bytes());

        self.send_tcp(download_message.as_slice(), true)?;

        Ok(())
    }

    pub fn send_download_fail(
        &mut self,
        download_fail: &FileFail,
    ) -> Result<(), QuickTransferError> {
        let mut download_fail_message = MESSAGE_DOWNLOAD_FAIL.as_bytes().to_vec();
        let answer =
            bincode::serialize(download_fail).map_err(|_| QuickTransferError::FatalError)?;

        // We assume that usize <= u64:
        download_fail_message
            .write_u64::<BE>(answer.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::FatalError)?;

        download_fail_message.extend(answer);

        self.send_tcp(download_fail_message.as_slice(), true)?;

        Ok(())
    }

    pub fn send_download_success(&mut self, file_size: u64) -> Result<(), QuickTransferError> {
        let mut download_success_message = MESSAGE_DOWNLOAD_SUCCESS.as_bytes().to_vec();

        // We assume that usize <= u64:
        download_success_message
            .write_u64::<BE>(file_size)
            .map_err(|_| QuickTransferError::FatalError)?;

        self.send_tcp(download_success_message.as_slice(), true)?;

        Ok(())
    }

    pub fn send_file(
        &mut self,
        mut file: File,
        file_size: u64,
        file_path: &Path,
    ) -> Result<(), QuickTransferError> {
        let mut bytes_to_send_left = file_size;
        let mut buffer = [0_u8; MAX_FILE_FRAGMENT_SIZE];
        while bytes_to_send_left > 0 {
            let read_bytes =
                file.read(&mut buffer)
                    .map_err(|_| QuickTransferError::ProblemReadingFile {
                        file_path: String::from(file_path.to_str().unwrap()),
                    })?;

            if read_bytes == 0 {
                break;
            }
            let read_bytes_u64 = read_bytes.try_into().unwrap();

            self.send_tcp(&buffer[..read_bytes], bytes_to_send_left <= read_bytes_u64)?;
            bytes_to_send_left -= read_bytes_u64;
        }

        if bytes_to_send_left > 0 {
            return Err(QuickTransferError::ProblemReadingFile {
                file_path: String::from(file_path.to_str().unwrap()),
            });
        }

        Ok(())
    }

    pub fn send_upload(
        &mut self,
        file: File,
        file_size: u64,
        file_name: &str,
        file_path: &Path,
    ) -> Result<(), QuickTransferError> {
        let mut upload_message = MESSAGE_UPLOAD.as_bytes().to_vec();

        // We assume that usize <= u64:
        upload_message
            .write_u64::<BE>(file_name.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::FatalError)?;

        upload_message.extend(file_name.as_bytes());

        // We assume that usize <= u64:
        upload_message
            .write_u64::<BE>(file_size)
            .map_err(|_| QuickTransferError::FatalError)?;

        self.send_tcp(upload_message.as_slice(), true)?;

        self.send_file(file, file_size, file_path)?;

        Ok(())
    }

    pub fn send_upload_fail(&mut self, upload_fail: FileFail) -> Result<(), QuickTransferError> {
        let mut upload_fail_message = MESSAGE_UPLOAD_RESULT.as_bytes().to_vec();
        let answer = bincode::serialize(&UploadResult::Fail(upload_fail))
            .map_err(|_| QuickTransferError::FatalError)?;

        // We assume that usize <= u64:
        upload_fail_message
            .write_u64::<BE>(answer.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::FatalError)?;

        upload_fail_message.extend(answer);

        self.send_tcp(upload_fail_message.as_slice(), true)?;

        Ok(())
    }

    pub fn send_upload_success(&mut self) -> Result<(), QuickTransferError> {
        let mut upload_success = MESSAGE_UPLOAD_RESULT.as_bytes().to_vec();
        let answer = bincode::serialize(&UploadResult::Success)
            .map_err(|_| QuickTransferError::FatalError)?;

        // We assume that usize <= u64:
        upload_success
            .write_u64::<BE>(answer.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::FatalError)?;

        upload_success.extend(answer);

        self.send_tcp(upload_success.as_slice(), true)?;

        Ok(())
    }
    pub fn send_disconnect_message(&mut self) -> Result<(), QuickTransferError> {
        self.send_tcp(MESSAGE_DISCONNECT.as_bytes(), true)?;

        Ok(())
    }
}
