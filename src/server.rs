use colored::*;
// use rustyline::{error::ReadlineError, history::History, DefaultEditor};
use std::fs::{self, File};
use std::sync::atomic::AtomicBool;
use std::io::{ErrorKind, Read, Write};
use std::sync::{Arc, Mutex};
//use std::net::{TcpListener, TcpStream};
use tokio::net::{TcpListener, TcpStream};
use::tokio::sync::mpsc::{self, Sender};
use std::path::{Path, PathBuf};
use rustyline_async::{Readline, ReadlineError, ReadlineEvent};

use crate::common::messages::{
    CdAnswer, FileFail, MESSAGE_CD, MESSAGE_DISCONNECT, MESSAGE_DOWNLOAD, MESSAGE_INIT, MESSAGE_LS,
    MESSAGE_UPLOAD,
};
use crate::common::{
    directory_description, CommunicationAgent, ProgramOptions, ProgramRole, QuickTransferError, ServerCommand,
};

/// This functions server program run in server mode.
pub async fn handle_server(program_options: ProgramOptions) -> Result<(), QuickTransferError> {
    println!(
        // "Welcome to QuickTransfer!\nTo exit, type `exit`.\nWaiting for clients to connect on port {}...",
        "Welcome to QuickTransfer!\nWaiting for clients to connect on port {} (interface {})...",
        program_options.port, program_options.server_ip_address,
    );

    let listener = create_a_listener(&program_options).await?;

    // For now, the server operates one client and exits.
    // The documentation says that iterator incoming will never return none, hence we can unwrap it:
    //let stream = listener.incoming().next().unwrap();
    let incoming = listener.accept().await.map_err(|_| QuickTransferError::ConnectionCreation)?;
    handle_client_as_a_server(incoming.0, &program_options).await?;

    Ok(())
}

/// Creates a TCP listener for server.
async fn create_a_listener(program_options: &ProgramOptions) -> Result<TcpListener, QuickTransferError> {
    let listener = TcpListener::bind((
        program_options.server_ip_address.clone(),
        program_options.port,
    ));

    listener.await.map_err(|_| QuickTransferError::ServerCreation)
    //listener
}

/// Handles the client once it is connected on some TCP stream.
async fn handle_client_as_a_server(
    mut stream: TcpStream,
    program_options: &ProgramOptions,
) -> Result<(), QuickTransferError> {
    // The documentation doesn't say, when this functions returns an error, so let's assume that never:
    let client_address = stream.peer_addr().unwrap();
    let mut agent = CommunicationAgent::new(&mut stream, ProgramRole::Server);

    agent.receive_message_header_check(MESSAGE_INIT).await?;

    let client_name = client_address.ip().to_canonical().to_string();

    println!(
        "{}{}{}",
        "A new client (".green().bold(),
        client_name.on_green().white(),
        ") has connected!".green().bold(),
    );

    let mut current_path = PathBuf::new();
    current_path.push(&program_options.root_directory);
    current_path = current_path.canonicalize().unwrap();
    let root_directory = current_path.as_path().canonicalize().unwrap();

    agent.send_directory_description(&current_path, &root_directory).await?;

    let rl = Readline::new(String::from("QuickTransfer> ")).unwrap();
    let mut writer = rl.1;
    let mut rl = rl.0;

    // TODO: pozamieniać te println na pisanie za pomocą writera
    loop {
        tokio::select! {
            command = rl.readline() => {
                match command {
                    Err(err) => {
                        return Err(QuickTransferError::ReadLineError {
                            error: err.to_string(),
                        });
                    }
                    Ok(ReadlineEvent::Eof) => {
                        eprintln!("^D");
                        return Ok(());
                    }
                    Ok(ReadlineEvent::Interrupted) => {
                        eprintln!("^C");
                        return Ok(());
                    }
                    Ok(ReadlineEvent::Line(ref line)) => {
                        rl.add_history_entry(line.to_string());
    
                        match line.as_str() {
                            "exit" | "disconnect" | "quit" => {
                                agent.send_disconnect_message().await?;
                                return Ok(())
                            }
                            "help" => {
                                println!("Help will be there...");
                                // TODO: write help
                            }
                            command => {
                                write!(
                                    writer,
                                    "{}{}{}",
                                    "Error: Command `".red(),
                                    command.red(),
                                    "` does not exist!".red(),
                                ).map_err(|_| QuickTransferError::StdoutError)?;
                            }
                        }
                    }
                }
            }
            header_received = agent.receive_message_header() => {
                let header_received = header_received?;
                match header_received.as_str() {
                    MESSAGE_CD => {
                        let dir_name = agent.receive_cd_message().await?;
                        let mut next_path = current_path.to_path_buf();
                        next_path.push(dir_name);
        
                        if !fs::exists(next_path.as_path()).unwrap() || !next_path.as_path().is_dir() {
                            agent.send_cd_answer(&CdAnswer::DirectoryDoesNotExist).await?;
                            continue;
                        }
        
                        let next_path = next_path.canonicalize().unwrap();
                        if !next_path.starts_with(root_directory.clone()) || next_path == current_path {
                            agent.send_cd_answer(&CdAnswer::IllegalDirectory).await?;
                            continue;
                        }
        
                        current_path = next_path;
        
                        let directory_contents = directory_description(&current_path, &root_directory)?;
                        agent.send_cd_answer(&CdAnswer::Success(directory_contents)).await?;
                    }
                    MESSAGE_LS => {
                        agent.send_directory_description(&current_path, &root_directory).await?;
                    }
                    MESSAGE_DOWNLOAD => {
                        let file_name = agent.receive_length_with_string().await?;
                        let mut file_path = current_path.to_path_buf();
                        file_path.push(file_name);
        
                        if !fs::exists(file_path.as_path()).unwrap() || !file_path.as_path().is_file() {
                            agent.send_download_fail(&FileFail::FileDoesNotExist).await?;
                            continue;
                        }
        
                        let current = file_path.canonicalize().unwrap();
                        if !current.starts_with(root_directory.clone()) {
                            agent.send_download_fail(&FileFail::IllegalFile).await?;
                            continue;
                        }
        
                        let opened_file =
                            File::open(&file_path).map_err(|_| QuickTransferError::ProblemOpeningFile {
                                file_path: String::from(file_path.to_str().unwrap()),
                            })?;
        
                        let file_size = opened_file.metadata().unwrap().len();
                        agent.send_download_success(file_size).await?;
        
                        agent.send_file(opened_file, file_size, &file_path).await?;
                    }
                    MESSAGE_UPLOAD => {
                        let file_name = agent.receive_length_with_string().await?;
                        let file_size = agent.receive_message_length().await?;
                        let mut file_path = current_path.to_path_buf();
                        file_path.push(&file_name);
        
                        let file_name_truncated = file_name.split("/").last().unwrap_or(&file_name);
        
                        let file_path = Path::new(file_name_truncated);
                        let opened_file = File::create(file_name_truncated);
        
                        let mut fail = false;
                        if opened_file.is_err() {
                            fail = true;
                            agent.send_upload_fail(FileFail::ErrorCreatingFile).await?;
                        }
        
                        agent.receive_file(opened_file.unwrap(), file_size, file_path, !fail).await?;
                        if !fail {
                            agent.send_upload_success().await?;
                        }
                    }
                    MESSAGE_DISCONNECT => {
                        println!(
                            "{}{}{}",
                            "Client (".green().bold(),
                            client_name.on_green().white(),
                            ") has disconnected.".green().bold(),
                        );

                        return Ok(());
                    }
                    _ => {
                        println!(
                            "{}{}{}",
                            "Client `".red(),
                            client_name.red(),
                            "` sent an invalid message. Disconnecting...".red(),
                        );
                        return Ok(());
                    }
                }
            }
        }
    }
}


async fn server_task(agent: &mut CommunicationAgent<'_>, client_name: &String, shutdown_tx: Sender<()>, program_options: &ProgramOptions) -> Result<(), QuickTransferError> {
    

    Ok(())
}
