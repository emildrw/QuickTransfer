use colored::*;
use rustyline_async::{Readline, ReadlineEvent, SharedWriter};
use std::{
    fs::{self, File},
    io::{ErrorKind, Write},
    ops::Deref,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::broadcast::{self, Receiver, Sender},
};

use crate::common::{
    directory_description,
    messages::{
        CdAnswer, FileFail, MkdirAnswer, RemoveAnswer, RenameAnswer, UploadResult, MESSAGE_CD,
        MESSAGE_CDANSWER, MESSAGE_DISCONNECT, MESSAGE_DOWNLOAD, MESSAGE_DOWNLOAD_FAIL,
        MESSAGE_INIT, MESSAGE_LS, MESSAGE_MKDIR, MESSAGE_MKDIRANS, MESSAGE_REMOVE,
        MESSAGE_REMOVE_ANSWER, MESSAGE_RENAME, MESSAGE_RENAME_ANSWER, MESSAGE_UPLOAD,
        MESSAGE_UPLOAD_RESULT,
    },
    CommunicationAgent, ProgramOptions, ProgramRole, QuickTransferError,
};

/// This functions server program run in server mode.
pub async fn handle_server(program_options: ProgramOptions) -> Result<(), QuickTransferError> {
    println!(
        "Welcome to QuickTransfer!\nTo exit, type `exit`.\nWaiting for clients to connect on port {} (interface {})...",
        program_options.port, program_options.server_ip_address,
    );

    let listener = create_a_listener(&program_options).await?;
    let program_options_arc = Arc::new(program_options);

    // .0 -> true iff stop thread listening for next clients, .1 -> true iff server was closed from server
    let (tx_stop, mut rx_stop) = broadcast::channel(1);
    let tx_stop2 = tx_stop.clone();
    // -> true iff server was closed from server
    let (tx_disconnected, mut rx_disconnected) = broadcast::channel(1);

    let rl = Readline::new(String::from("QuickTransfer> ")).unwrap();
    let mut writer = rl.1;
    let mut rl = rl.0;
    let writer2 = writer.clone();

    let connected_clients = Arc::new(AtomicUsize::new(0));
    let connected_clients2 = Arc::clone(&connected_clients);

    tokio::spawn(async move {
        // Pre-print user help:
        let mut user_help = String::new();
        preprint_user_help(&mut user_help);

        loop {
            tokio::select! {
                closed_from_server = rx_disconnected.recv() => {
                    connected_clients.fetch_sub(1, Ordering::Relaxed);
                    rl.flush().map_err(|_| QuickTransferError::Stdout)?;

                    if check_clients_number_and_stop(&connected_clients, &tx_stop, closed_from_server.unwrap())? {
                        return Ok(())
                    }
                }
                command = rl.readline() => {
                    match command {
                        Err(err) => {
                            return Err(QuickTransferError::ReadLine {
                                error: err.to_string(),
                            });
                        }
                        Ok(ReadlineEvent::Eof) => {
                            eprintln!("^D");
                            if check_clients_number_and_stop(&connected_clients, &tx_stop, true)? {
                                return Ok(())
                            } else {
                                tx_stop.send((false, true)).unwrap();
                            }
                        }
                        Ok(ReadlineEvent::Interrupted) => {
                            eprintln!("^C");
                            if check_clients_number_and_stop(&connected_clients, &tx_stop, true)? {
                                return Ok(())
                            } else {
                                tx_stop.send((false, true)).unwrap();
                            }
                        }
                        Ok(ReadlineEvent::Line(ref line)) => {
                            rl.add_history_entry(line.to_string());

                            let input = line.trim();
                            let mut input_splitted = input.split_whitespace();
                            let command = input_splitted.next();

                            match command {
                                Some("clear") => {
                                    rl.clear().map_err(|_| QuickTransferError::Stdout)?;
                                }
                                Some("exit") | Some("disconnect") | Some("quit") => {
                                    tx_stop.send((false, true)).unwrap();
                                }
                                Some("help") => {
                                    Write::write(&mut writer, user_help.as_bytes()).map_err(|_| QuickTransferError::Stdout)?;
                                }
                                Some(command) => {
                                    writeln!(
                                        writer,
                                        "{}{}{}",
                                        "Error: Command `".red(),
                                        command.red(),
                                        "` does not exist!".red(),
                                    ).map_err(|_| QuickTransferError::Stdout)?;
                                }
                                None => {

                                }
                            }
                        }
                    }
                }
            }
        }
    });

    loop {
        tokio::select! {
            message = rx_stop.recv() => {
                let message = message.unwrap();
                if message.0 {
                    break;
                }
            }
            stream = listener.accept() => {
                let stream = stream.map_err(|_| QuickTransferError::ConnectionCreation)?.0;
                connected_clients2.fetch_add(1, Ordering::Relaxed);

                let program_options_arc = Arc::clone(&program_options_arc);
                let rx_stop = tx_stop2.subscribe();
                let tx_disconnected = tx_disconnected.clone();
                let mut writer = writer2.clone();

                tokio::spawn(async move {
                    let result = handle_client_as_a_server(stream, program_options_arc.deref(), tx_disconnected.clone(), rx_stop, &mut writer).await;
                    if let Err(error) = result {
                        tx_disconnected.send(true).unwrap();
                        eprintln!("{}", error);
                    }

                    Ok::<(), QuickTransferError>(())
                });
            }
        }
    }

    Ok(())
}

/// Checks whether there are 0 clients and returns whether the server should be stopped.
fn check_clients_number_and_stop(
    connected_clients: &Arc<AtomicUsize>,
    tx_stop: &Sender<(bool, bool)>,
    closed_from_server: bool,
) -> Result<bool, QuickTransferError> {
    if connected_clients.load(Ordering::Relaxed) == 0 {
        tx_stop.send((true, closed_from_server)).unwrap();
        if !closed_from_server {
            println!("{}", "All clients have disconnected.".green().bold());
        }

        return Ok(true);
    }

    Ok(false)
}

/// Creates a TCP listener for server.
async fn create_a_listener(
    program_options: &ProgramOptions,
) -> Result<TcpListener, QuickTransferError> {
    let listener = TcpListener::bind((
        program_options.server_ip_address.clone(),
        program_options.port,
    ));

    listener
        .await
        .map_err(|_| QuickTransferError::ServerCreation)
}

/// Handles the client once it is connected on some TCP stream.
async fn handle_client_as_a_server(
    mut stream: TcpStream,
    program_options: &ProgramOptions,
    tx_disconnected: Sender<bool>,
    mut rx_stop: Receiver<(bool, bool)>,
    writer: &mut SharedWriter,
) -> Result<(), QuickTransferError> {
    // The documentation doesn't say, when this functions returns an error, so let's assume that never:
    let client_address = stream.peer_addr().unwrap();
    let mut agent =
        CommunicationAgent::new(&mut stream, ProgramRole::Server, program_options.timeout);

    agent.receive_message_header_check(MESSAGE_INIT).await?;

    let client_name = client_address.ip().to_canonical().to_string();
    let client_port = client_address.port();

    writeln!(
        writer,
        "{}{}{}",
        "A new client (".green().bold(),
        format!("[{}]:{}", client_name, client_port,)
            .on_green()
            .white(),
        ") has connected!".green().bold(),
    )
    .map_err(|_| QuickTransferError::Stdout)?;

    let mut current_path = PathBuf::new();
    current_path.push(&program_options.root_directory);
    current_path = current_path.canonicalize().unwrap();
    let root_directory = current_path.as_path().canonicalize().unwrap();

    agent
        .send_directory_description(&current_path, &root_directory)
        .await?;

    loop {
        tokio::select! {
            message = rx_stop.recv() => {
                let message = message.unwrap();

                if !message.0 {
                    agent.send_disconnect_message().await?;
                    tx_disconnected.send(message.1).unwrap();

                    return Ok(());
                }
            }
            header_received = agent.receive_message_header() => {
                let header_received = header_received?;
                match header_received.as_str() {
                    MESSAGE_CD => {
                        let dir_name = agent.receive_cd_message().await?;
                        let mut next_path = current_path.to_path_buf();
                        next_path.push(dir_name);

                        if !fs::exists(next_path.as_path()).unwrap_or(false) || !next_path.as_path().is_dir() {
                            agent.send_answer(MESSAGE_CDANSWER, &CdAnswer::DirectoryDoesNotExist).await?;
                            continue;
                        }

                        let next_path = next_path.canonicalize().unwrap();
                        if !next_path.starts_with(root_directory.clone()) || next_path == current_path {
                            agent.send_answer(MESSAGE_CDANSWER, &CdAnswer::IllegalDirectory).await?;
                            continue;
                        }

                        current_path = next_path;

                        let Ok(directory_contents) = directory_description(&current_path, &root_directory) else {
                            agent.send_answer(MESSAGE_CDANSWER, &CdAnswer::ReadingDirectoryError).await?;
                            continue;
                        };
                        agent.send_answer(MESSAGE_CDANSWER, &CdAnswer::Success(directory_contents)).await?;
                    }
                    MESSAGE_LS => {
                        agent.send_directory_description(&current_path, &root_directory).await?;
                    }
                    MESSAGE_DOWNLOAD => {
                        let file_name = agent.receive_length_with_string().await?;
                        let mut file_path = current_path.to_path_buf();
                        file_path.push(file_name);

                        if !fs::exists(file_path.as_path()).unwrap() || !file_path.as_path().is_file() {
                            agent.send_answer(MESSAGE_DOWNLOAD_FAIL, &FileFail::FileDoesNotExist).await?;
                            continue;
                        }

                        let current = file_path.canonicalize().unwrap();
                        if !current.starts_with(root_directory.clone()) {
                            agent.send_answer(MESSAGE_DOWNLOAD_FAIL, &FileFail::IllegalFile).await?;
                            continue;
                        }

                        let Ok(opened_file) = File::open(&file_path) else {
                            agent.send_answer(MESSAGE_DOWNLOAD_FAIL, &FileFail::ErrorOpeningFile).await?;
                            continue;
                        };

                        let file_size = opened_file.metadata().unwrap().len();
                        agent.send_download_success(file_size).await?;

                        agent.send_file(opened_file, file_size, &file_path).await?;
                    }
                    MESSAGE_UPLOAD => {
                        let file_name = agent.receive_length_with_string().await?;
                        let file_size = agent.receive_message_length().await?;
                        let mut file_path = current_path.to_path_buf();
                        let file_name_truncated = Path::new(&file_name).file_name().map(|string| string.to_str().map(|string| string.to_string())).unwrap_or(Some(file_name.clone())).unwrap_or(file_name.clone());
                        file_path.push(&file_name_truncated);

                        let file_path = file_path.as_path();
                        let opened_file = File::create(file_path);

                        let mut fail = false;
                        if opened_file.is_err() {
                            fail = true;
                            agent.send_answer(MESSAGE_UPLOAD_RESULT, &UploadResult::Fail(FileFail::ErrorCreatingFile)).await?;
                        }

                        agent.receive_file(opened_file.unwrap(), file_size, file_path, !fail).await?;
                        if !fail {
                            agent.send_answer(MESSAGE_UPLOAD_RESULT, &UploadResult::Success).await?;
                        }
                    }
                    MESSAGE_MKDIR => {
                        let directory_name = agent.receive_length_with_string().await?;
                        let mut next_path = current_path.to_path_buf();
                        next_path.push(&directory_name);

                        if fs::exists(next_path.as_path()).unwrap() {
                            agent.send_answer(MESSAGE_MKDIRANS, &MkdirAnswer::DirectoryAlreadyExists).await?;
                            continue;
                        }

                        if !next_path.starts_with(root_directory.clone()) || next_path == current_path {
                            agent.send_answer(MESSAGE_MKDIRANS, &MkdirAnswer::IllegalDirectory).await?;
                            continue;
                        }


                        if fs::create_dir(&next_path).is_err() {
                            agent.send_answer(MESSAGE_MKDIRANS, &MkdirAnswer::ErrorCreatingDirectory).await?;
                            continue;
                        }

                        agent.send_answer(MESSAGE_MKDIRANS, &MkdirAnswer::Success).await?;
                    }
                    MESSAGE_RENAME => {
                        let file_dir_name = agent.receive_length_with_string().await?;
                        let new_name = agent.receive_length_with_string().await?;

                        let mut file_path = current_path.to_path_buf();
                        file_path.push(&file_dir_name);

                        if !fs::exists(file_path.as_path()).unwrap() {
                            agent.send_answer(MESSAGE_RENAME_ANSWER, &RenameAnswer::FileDirDoesNotExist).await?;
                            continue;
                        }

                        let current = file_path.canonicalize().unwrap();
                        if !current.starts_with(root_directory.clone()) {
                            agent.send_answer(MESSAGE_RENAME_ANSWER, &RenameAnswer::IllegalFileDir).await?;
                            continue;
                        }

                        if fs::rename(&file_path, new_name).is_err() {
                            agent.send_answer(MESSAGE_RENAME_ANSWER, &RenameAnswer::ErrorRenaming).await?;
                            continue;
                        }

                        agent.send_answer(MESSAGE_RENAME_ANSWER, &RenameAnswer::Success).await?;
                    }
                    MESSAGE_REMOVE => {
                        let file_dir_name = agent.receive_length_with_string().await?;

                        let mut file_path = current_path.to_path_buf();
                        file_path.push(&file_dir_name);

                        if !fs::exists(file_path.as_path()).unwrap() {
                            agent.send_answer(MESSAGE_REMOVE_ANSWER, &RemoveAnswer::FileDirDoesNotExist).await?;
                            continue;
                        }

                        let current = file_path.canonicalize().unwrap();
                        if !current.starts_with(root_directory.clone()) {
                            agent.send_answer(MESSAGE_REMOVE_ANSWER, &RemoveAnswer::IllegalFileDir).await?;
                            continue;
                        }

                        if file_path.is_dir() {
                            if let Err(err) = fs::remove_dir(&file_path) {
                                if err.kind() == ErrorKind::DirectoryNotEmpty {
                                    agent.send_answer(MESSAGE_REMOVE_ANSWER, &RemoveAnswer::DirectoryNotEmpty).await?;
                                } else {
                                    agent.send_answer(MESSAGE_REMOVE_ANSWER, &RemoveAnswer::ErrorRemoving).await?;
                                }

                                continue;
                            }
                        } else if fs::remove_file(&file_path).is_err() {
                            agent.send_answer(MESSAGE_REMOVE_ANSWER, &RemoveAnswer::ErrorRemoving).await?;
                            continue;
                        }

                        agent.send_answer(MESSAGE_REMOVE_ANSWER, &RemoveAnswer::Success).await?;
                    }
                    MESSAGE_DISCONNECT => {
                        writeln!(
                            writer,
                            "{}{}{}",
                            "Client (".green().bold(),
                            format!(
                                "[{}]:{}",
                                client_name,
                                client_port,
                            ).on_green().white(),
                            ") has disconnected.".green().bold(),
                        ).map_err(|_| QuickTransferError::Stdout)?;

                        tx_disconnected.send(false).unwrap();

                        return Ok(());
                    }
                    _ => {
                        eprintln!(
                            "{}{}{}",
                            "Client (".red(),
                            format!(
                                "[{}]:{}",
                                client_name,
                                client_port,
                            ).on_red().white(),
                            ") sent an invalid message. Disconnecting...".red(),
                        );

                        tx_disconnected.send(false).unwrap();

                        return Ok(());
                    }
                }
            }
        }
    }
}

/// Pre-prints user help so as not to do it every time.
fn preprint_user_help(help_msg: &mut String) {
    help_msg.push_str("Available commands:\n");
    help_msg.push_str("  clear                          Clear the screen.\n");
    help_msg.push_str("  exit; disconnect; quit         Gracefully disconnect all clients\n");
    help_msg.push_str("                                 and exit QuickTransfer.\n");
}
