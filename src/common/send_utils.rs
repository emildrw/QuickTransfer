use byteorder::{WriteBytesExt, BE};
use std::{
    fs::File,
    io::{ErrorKind, Read},
    path::Path,
};
use tokio::io::AsyncWriteExt;

use crate::common::{
    directory_description,
    messages::{
        CdAnswer, FileFail, MkdirAnswer, UploadResult, MAX_FILE_FRAGMENT_SIZE, MESSAGE_CD,
        MESSAGE_CDANSWER, MESSAGE_DIR, MESSAGE_DISCONNECT, MESSAGE_DOWNLOAD, MESSAGE_DOWNLOAD_FAIL,
        MESSAGE_DOWNLOAD_SUCCESS, MESSAGE_INIT, MESSAGE_LS, MESSAGE_MKDIR, MESSAGE_MKDIRANS,
        MESSAGE_UPLOAD, MESSAGE_UPLOAD_RESULT,
    },
    CommunicationAgent, QuickTransferError,
};

use super::messages::{RenameAnswer, MESSAGE_RENAME, MESSAGE_RENAME_ANSWER};

impl CommunicationAgent<'_> {
    /// Send bytes from message over TCP.
    async fn send_tcp(&mut self, message: &[u8], flush: bool) -> Result<(), QuickTransferError> {
        self.stream.write_all(message).await.map_err(|err| {
            if let ErrorKind::UnexpectedEof = err.kind() {
                QuickTransferError::RemoteClosedConnection(self.role)
            } else {
                QuickTransferError::ErrorWhileSendingMessage(self.role)
            }
        })?;

        if flush {
            self.stream
                .flush()
                .await
                .map_err(|_| QuickTransferError::ErrorWhileSendingMessage(self.role))?;
        }

        Ok(())
    }

    /// Sends an init message (header).
    pub async fn send_init_message(&mut self) -> Result<(), QuickTransferError> {
        self.send_tcp(MESSAGE_INIT.as_bytes(), true).await?;

        Ok(())
    }

    /// Sends directory description: header, description length, description.
    pub async fn send_directory_description(
        &mut self,
        directory_path: &Path,
        root_directory_path: &Path,
    ) -> Result<(), QuickTransferError> {
        let directory_contents = directory_description(directory_path, root_directory_path)?;

        let mut dir_message = MESSAGE_DIR.as_bytes().to_vec();

        let dir_description = bincode::serialize(&directory_contents)
            .map_err(|_| QuickTransferError::ReadingDirectoryContents)?;

        // We assume that usize <= u64:
        WriteBytesExt::write_u64::<BE>(&mut dir_message, dir_description.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::FatalError)?;

        dir_message.extend(dir_description);

        self.send_tcp(dir_message.as_slice(), true).await?;

        Ok(())
    }

    /// Sends change directory message: header, directory name length, directory length.
    pub async fn send_change_directory(
        &mut self,
        directory_name: &str,
    ) -> Result<(), QuickTransferError> {
        let mut cd_message = MESSAGE_CD.as_bytes().to_vec();

        // We assume that usize <= u64:
        WriteBytesExt::write_u64::<BE>(&mut cd_message, directory_name.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::FatalError)?;

        cd_message.extend(directory_name.as_bytes());

        self.send_tcp(cd_message.as_slice(), true).await?;

        Ok(())
    }

    // Send a change directory answer: header, answer length, answer.
    pub async fn send_cd_answer(&mut self, answer: &CdAnswer) -> Result<(), QuickTransferError> {
        let mut cdanswer_message = MESSAGE_CDANSWER.as_bytes().to_vec();
        let answer = bincode::serialize(answer).map_err(|_| QuickTransferError::FatalError)?;

        // We assume that usize <= u64:
        WriteBytesExt::write_u64::<BE>(&mut cdanswer_message, answer.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::FatalError)?;

        cdanswer_message.extend(answer);

        self.send_tcp(cdanswer_message.as_slice(), true).await?;

        Ok(())
    }

    /// Sends a `ls` message (header).
    pub async fn send_list_directory(&mut self) -> Result<(), QuickTransferError> {
        self.send_tcp(MESSAGE_LS.as_bytes(), true).await?;

        Ok(())
    }

    /// Sends file download request: header, file name length, file name.
    pub async fn send_download_request(
        &mut self,
        file_name: &str,
    ) -> Result<(), QuickTransferError> {
        let mut download_message = MESSAGE_DOWNLOAD.as_bytes().to_vec();

        // We assume that usize <= u64:
        WriteBytesExt::write_u64::<BE>(&mut download_message, file_name.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::FatalError)?;

        download_message.extend(file_name.as_bytes());

        self.send_tcp(download_message.as_slice(), true).await?;

        Ok(())
    }

    /// Sends download fail message: header, message length, message.
    pub async fn send_download_fail(
        &mut self,
        download_fail: &FileFail,
    ) -> Result<(), QuickTransferError> {
        let mut download_fail_message = MESSAGE_DOWNLOAD_FAIL.as_bytes().to_vec();
        let answer =
            bincode::serialize(download_fail).map_err(|_| QuickTransferError::FatalError)?;

        // We assume that usize <= u64:
        WriteBytesExt::write_u64::<BE>(
            &mut download_fail_message,
            answer.len().try_into().unwrap(),
        )
        .map_err(|_| QuickTransferError::FatalError)?;

        download_fail_message.extend(answer);

        self.send_tcp(download_fail_message.as_slice(), true)
            .await?;

        Ok(())
    }

    /// Sends download success message: header, file size (in bytes) (without file contents!)
    pub async fn send_download_success(
        &mut self,
        file_size: u64,
    ) -> Result<(), QuickTransferError> {
        let mut download_success_message = MESSAGE_DOWNLOAD_SUCCESS.as_bytes().to_vec();

        // We assume that usize <= u64:
        WriteBytesExt::write_u64::<BE>(&mut download_success_message, file_size)
            .map_err(|_| QuickTransferError::FatalError)?;

        self.send_tcp(download_success_message.as_slice(), true)
            .await?;

        Ok(())
    }

    /// Sends a file (only bytes from that file) in blocks.
    pub async fn send_file(
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

            self.send_tcp(&buffer[..read_bytes], bytes_to_send_left <= read_bytes_u64)
                .await?;
            bytes_to_send_left -= read_bytes_u64;
        }

        if bytes_to_send_left > 0 {
            return Err(QuickTransferError::ProblemReadingFile {
                file_path: String::from(file_path.to_str().unwrap()),
            });
        }

        Ok(())
    }

    /// Sends an upload message: header, file size (in bytes), file contents.
    pub async fn send_upload(
        &mut self,
        file: File,
        file_size: u64,
        file_name: &str,
        file_path: &Path,
    ) -> Result<(), QuickTransferError> {
        let mut upload_message = MESSAGE_UPLOAD.as_bytes().to_vec();

        // We assume that usize <= u64:
        WriteBytesExt::write_u64::<BE>(&mut upload_message, file_name.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::FatalError)?;

        upload_message.extend(file_name.as_bytes());

        // We assume that usize <= u64:
        WriteBytesExt::write_u64::<BE>(&mut upload_message, file_size)
            .map_err(|_| QuickTransferError::FatalError)?;

        self.send_tcp(upload_message.as_slice(), true).await?;

        self.send_file(file, file_size, file_path).await?;

        Ok(())
    }

    /// Sends upload fail message: header, answer length, answer.
    pub async fn send_upload_fail(
        &mut self,
        upload_fail: FileFail,
    ) -> Result<(), QuickTransferError> {
        let mut upload_fail_message = MESSAGE_UPLOAD_RESULT.as_bytes().to_vec();
        let answer = bincode::serialize(&UploadResult::Fail(upload_fail))
            .map_err(|_| QuickTransferError::FatalError)?;

        // We assume that usize <= u64:
        WriteBytesExt::write_u64::<BE>(&mut upload_fail_message, answer.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::FatalError)?;

        upload_fail_message.extend(answer);

        self.send_tcp(upload_fail_message.as_slice(), true).await?;

        Ok(())
    }

    // Sends upload success message: header, answer length, answer.
    pub async fn send_upload_success(&mut self) -> Result<(), QuickTransferError> {
        let mut upload_success = MESSAGE_UPLOAD_RESULT.as_bytes().to_vec();
        let answer = bincode::serialize(&UploadResult::Success)
            .map_err(|_| QuickTransferError::FatalError)?;

        // We assume that usize <= u64:
        WriteBytesExt::write_u64::<BE>(&mut upload_success, answer.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::FatalError)?;

        upload_success.extend(answer);

        self.send_tcp(upload_success.as_slice(), true).await?;

        Ok(())
    }

    /// Sends an mkdir message: header, name length, name.
    pub async fn send_mkdir(&mut self, directory_name: &str) -> Result<(), QuickTransferError> {
        let mut mkdir_message = MESSAGE_MKDIR.as_bytes().to_vec();

        // We assume that usize <= u64:
        WriteBytesExt::write_u64::<BE>(
            &mut mkdir_message,
            directory_name.len().try_into().unwrap(),
        )
        .map_err(|_| QuickTransferError::FatalError)?;

        mkdir_message.extend(directory_name.as_bytes());

        self.send_tcp(mkdir_message.as_slice(), true).await?;

        Ok(())
    }

    // Send a mkdir answer: header, answer length, answer.
    pub async fn send_mkdir_answer(
        &mut self,
        answer: &MkdirAnswer,
    ) -> Result<(), QuickTransferError> {
        let mut mkdir_answer_message = MESSAGE_MKDIRANS.as_bytes().to_vec();
        let answer = bincode::serialize(answer).map_err(|_| QuickTransferError::FatalError)?;

        // We assume that usize <= u64:
        WriteBytesExt::write_u64::<BE>(&mut mkdir_answer_message, answer.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::FatalError)?;

        mkdir_answer_message.extend(answer);

        self.send_tcp(mkdir_answer_message.as_slice(), true).await?;

        Ok(())
    }

    /// Sends rename request: header, file name length, file name.
    pub async fn send_rename_request(
        &mut self,
        file_name: &str,
        new_name: &str,
    ) -> Result<(), QuickTransferError> {
        let mut rename_message = MESSAGE_RENAME.as_bytes().to_vec();

        // We assume that usize <= u64:
        WriteBytesExt::write_u64::<BE>(&mut rename_message, file_name.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::FatalError)?;

        rename_message.extend(file_name.as_bytes());

        // We assume that usize <= u64:
        WriteBytesExt::write_u64::<BE>(&mut rename_message, new_name.len().try_into().unwrap())
            .map_err(|_| QuickTransferError::FatalError)?;

        rename_message.extend(new_name.as_bytes());

        self.send_tcp(rename_message.as_slice(), true).await?;

        Ok(())
    }

    // Send a rename answer: header, answer length, answer.
    pub async fn send_rename_answer(
        &mut self,
        answer: &RenameAnswer,
    ) -> Result<(), QuickTransferError> {
        let mut rename_answer_message = MESSAGE_RENAME_ANSWER.as_bytes().to_vec();
        let answer = bincode::serialize(answer).map_err(|_| QuickTransferError::FatalError)?;

        // We assume that usize <= u64:
        WriteBytesExt::write_u64::<BE>(
            &mut rename_answer_message,
            answer.len().try_into().unwrap(),
        )
        .map_err(|_| QuickTransferError::FatalError)?;

        rename_answer_message.extend(answer);

        self.send_tcp(rename_answer_message.as_slice(), true)
            .await?;

        Ok(())
    }

    /// Sends a disconnect message (header).
    pub async fn send_disconnect_message(&mut self) -> Result<(), QuickTransferError> {
        self.send_tcp(MESSAGE_DISCONNECT.as_bytes(), true).await?;

        Ok(())
    }
}
