use colored::*;
use std::fs::{self, File};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

use crate::common::messages::{
    CdAnswer, FileFail, MESSAGE_CD, MESSAGE_DISCONNECT, MESSAGE_DOWNLOAD, MESSAGE_INIT, MESSAGE_LS,
    MESSAGE_UPLOAD,
};
use crate::common::{
    directory_description, CommunicationAgent, ProgramOptions, ProgramRole, QuickTransferError,
};

/// This functions server program run in server mode.
pub fn handle_server(program_options: ProgramOptions) -> Result<(), QuickTransferError> {
    println!(
        // "Welcome to QuickTransfer!\nTo exit, type `exit`.\nWaiting for clients to connect on port {}...",
        "Welcome to QuickTransfer!\nWaiting for clients to connect on port {} (interface {})...",
        program_options.port, program_options.server_ip_address,
    );

    let listener = create_a_listener(&program_options)?;

    // For now, the server operates one client and exits.
    let stream = listener.incoming().next().unwrap();
    handle_client_as_a_server(stream.unwrap(), &program_options)?;

    Ok(())
}

/// Creates a TCP listener foo server.
fn create_a_listener(program_options: &ProgramOptions) -> Result<TcpListener, QuickTransferError> {
    let listener = TcpListener::bind((
        program_options.server_ip_address.clone(),
        program_options.port,
    ));

    if listener.is_err() {
        return Err(QuickTransferError::ServerCreation);
    }

    Ok(listener.unwrap())
}

/// Handles a client once it is connected on some TCP stream.
fn handle_client_as_a_server(
    mut stream: TcpStream,
    program_options: &ProgramOptions,
) -> Result<(), QuickTransferError> {
    // The documentation doesn't say, when this functions returns an error, so let's assume that never:
    let client_address = stream.peer_addr().unwrap();
    let mut agent = CommunicationAgent::new(&mut stream, ProgramRole::Server);

    agent.receive_message_header_check(MESSAGE_INIT)?;

    let mut current_path = PathBuf::new();
    current_path.push(&program_options.root_directory);
    current_path = current_path.canonicalize().unwrap();
    let root_directory = current_path.as_path().canonicalize().unwrap();

    agent.send_directory_description(&current_path, &root_directory)?;

    let client_name = client_address.ip().to_canonical().to_string();

    println!(
        "{}",
        format!(
            "A new client ({}) has connected!",
            client_name.on_green().white()
        )
        .green()
        .bold()
    );

    loop {
        let header_received = agent.receive_message_header()?;
        match header_received.as_str() {
            MESSAGE_CD => {
                let dir_name = agent.receive_cd_message()?;
                let mut next_path = current_path.to_path_buf();
                next_path.push(dir_name);

                if !fs::exists(next_path.as_path()).unwrap() || !next_path.as_path().is_dir() {
                    agent.send_cd_answer(&CdAnswer::DirectoryDoesNotExist)?;
                    continue;
                }

                let next_path = next_path.canonicalize().unwrap();
                if !next_path.starts_with(root_directory.clone()) || next_path == current_path {
                    agent.send_cd_answer(&CdAnswer::IllegalDirectory)?;
                    continue;
                }

                current_path = next_path;

                let directory_contents = directory_description(&current_path, &root_directory)?;
                agent.send_cd_answer(&CdAnswer::Success(directory_contents))?;
            }
            MESSAGE_LS => {
                agent.send_directory_description(&current_path, &root_directory)?;
            }
            MESSAGE_DOWNLOAD => {
                let file_name = agent.receive_length_with_string()?;
                let mut file_path = current_path.to_path_buf();
                file_path.push(file_name);

                if !fs::exists(file_path.as_path()).unwrap() || !file_path.as_path().is_file() {
                    agent.send_download_fail(&FileFail::FileDoesNotExist)?;
                    continue;
                }

                let current = file_path.canonicalize().unwrap();
                if !current.starts_with(root_directory.clone()) {
                    agent.send_download_fail(&FileFail::IllegalFile)?;
                    continue;
                }

                let opened_file =
                    File::open(&file_path).map_err(|_| QuickTransferError::ProblemOpeningFile {
                        file_path: String::from(file_path.to_str().unwrap()),
                    })?;

                let file_size = opened_file.metadata().unwrap().len();
                agent.send_download_success(file_size)?;

                agent.send_file(opened_file, file_size, &file_path)?;
            }
            MESSAGE_UPLOAD => {
                let file_name = agent.receive_length_with_string()?;
                let file_size = agent.receive_message_length()?;
                let mut file_path = current_path.to_path_buf();
                file_path.push(&file_name);

                let file_name_truncated = file_name.split("/").last().unwrap_or(&file_name);

                let file_path = Path::new(file_name_truncated);
                let opened_file = File::create(file_name_truncated);

                let mut fail = false;
                if opened_file.is_err() {
                    fail = true;
                    agent.send_upload_fail(FileFail::ErrorCreatingFile)?;
                }

                agent.receive_file(opened_file.unwrap(), file_size, file_path, !fail)?;
                if !fail {
                    agent.send_upload_success()?;
                }
            }
            MESSAGE_DISCONNECT => {
                println!(
                    "{}",
                    format!(
                        "Client ({}) has disconnected.",
                        client_name.on_green().white()
                    )
                    .green()
                    .bold()
                );
                break;
            }
            _ => {
                println!(
                    "{}",
                    format!(
                        "Client `{}` sent an invalid message. Disconnecting...",
                        client_name
                    )
                    .red()
                );
                break;
            }
        }
    }

    Ok(())
}
