use colored::*;
use rustyline_async::{Readline, ReadlineEvent, SharedWriter};
use std::{
    fs::{self, File},
    io::Write,
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

use crate::common::messages::{
    CdAnswer, FileFail, MkdirAnswer, MESSAGE_CD, MESSAGE_DISCONNECT, MESSAGE_DOWNLOAD, MESSAGE_INIT, MESSAGE_LS, MESSAGE_MKDIR, MESSAGE_UPLOAD
};
use crate::common::{
    directory_description, CommunicationAgent, ProgramOptions, ProgramRole, QuickTransferError,
};

/// This functions server program run in server mode.
pub async fn handle_server(program_options: ProgramOptions) -> Result<(), QuickTransferError> {
    println!(
        "Welcome to QuickTransfer!\nTo exit, type `exit`.\nWaiting for clients to connect on port {} (interface {})...",
        program_options.port, program_options.server_ip_address,
    );

    let listener = create_a_listener(&program_options).await?;
    let program_options_arc = Arc::new(program_options);

    // Bool -> true iff stop thread listening for next clients
    let (tx_stop, mut rx_stop) = broadcast::channel(1);
    let tx_stop2 = tx_stop.clone();
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
                _ = rx_disconnected.recv() => {
                    connected_clients.fetch_sub(1, Ordering::Relaxed);

                    if check_clients_number_and_stop(&connected_clients, &tx_stop, true)? {
                        return Ok(())
                    }
                }
                command = rl.readline() => {
                    match command {
                        Err(err) => {
                            return Err(QuickTransferError::ReadLineError {
                                error: err.to_string(),
                            });
                        }
                        Ok(ReadlineEvent::Eof) => {
                            eprintln!("^D");
                            if check_clients_number_and_stop(&connected_clients, &tx_stop, false)? {
                                return Ok(())
                            } else {
                                tx_stop.send(false).unwrap();
                            }
                        }
                        Ok(ReadlineEvent::Interrupted) => {
                            eprintln!("^C");
                            if check_clients_number_and_stop(&connected_clients, &tx_stop, false)? {
                                return Ok(())
                            } else {
                                tx_stop.send(false).unwrap();
                            }
                        }
                        Ok(ReadlineEvent::Line(ref line)) => {
                            rl.add_history_entry(line.to_string());

                            let input = line.trim();
                            let mut input_splitted = input.split_whitespace();
                            let command = input_splitted.next();

                            match command {
                                Some("exit") | Some("disconnect") | Some("quit") => {
                                    tx_stop.send(false).unwrap();
                                }
                                Some("help") => {
                                    Write::write(&mut writer, user_help.as_bytes()).map_err(|_| QuickTransferError::StdoutError)?;
                                }
                                Some(command) => {
                                    writeln!(
                                        writer,
                                        "{}{}{}",
                                        "Error: Command `".red(),
                                        command.red(),
                                        "` does not exist!".red(),
                                    ).map_err(|_| QuickTransferError::StdoutError)?;
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
                if message {
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
                        tx_disconnected.send(()).unwrap();
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
fn check_clients_number_and_stop(connected_clients: &Arc<AtomicUsize>, tx_stop: &Sender<bool>, print_message: bool) -> Result<bool, QuickTransferError> {
    if connected_clients.load(Ordering::Relaxed) == 0 {
        tx_stop.send(true).unwrap();
        if print_message {
            println!(
                "\n{}",
                "All clients have disconnected.".green().bold(),
            );
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
    tx_disconnected: Sender<()>,
    mut rx_stop: Receiver<bool>,
    writer: &mut SharedWriter,
) -> Result<(), QuickTransferError> {
    // The documentation doesn't say, when this functions returns an error, so let's assume that never:
    let client_address = stream.peer_addr().unwrap();
    let mut agent = CommunicationAgent::new(&mut stream, ProgramRole::Server, program_options.timeout);

    let res = agent.receive_message_header_check(MESSAGE_INIT).await;
    if let Err(xd) = res {
        return Err(xd);
    }

    let client_name = client_address.ip().to_canonical().to_string();

    writeln!(
        writer,
        "{}{}{}",
        "A new client (".green().bold(),
        client_name.on_green().white(),
        ") has connected!".green().bold(),
    )
    .map_err(|_| QuickTransferError::StdoutError)?;

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

                if !message {
                    agent.send_disconnect_message().await?;
                    tx_disconnected.send(()).unwrap();

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
                    MESSAGE_MKDIR => {
                        let directory_name = agent.receive_length_with_string().await?;
                        let mut next_path = current_path.to_path_buf();
                        next_path.push(&directory_name);

                        if fs::exists(next_path.as_path()).unwrap() {
                            agent.send_mkdir_answer(&MkdirAnswer::DirectoryAlreadyExists).await?;
                            continue;
                        }

                        if !next_path.starts_with(root_directory.clone()) || next_path == current_path {
                            agent.send_mkdir_answer(&MkdirAnswer::ErrorCreatingDirectory).await?;
                            continue;
                        }

                        fs::create_dir(&next_path).map_err(|_| QuickTransferError::ProblemCreatingDirectory {
                            directory_name
                        })?;

                        agent.send_mkdir_answer(&MkdirAnswer::Success).await?;
                    }
                    MESSAGE_DISCONNECT => {
                        writeln!(
                            writer,
                            "{}{}{}",
                            "Client (".green().bold(),
                            client_name.on_green().white(),
                            ") has disconnected.".green().bold(),
                        ).map_err(|_| QuickTransferError::StdoutError)?;

                        writer.flush().map_err(|_| QuickTransferError::StdoutError)?;

                        tx_disconnected.send(()).unwrap();

                        return Ok(());
                    }
                    _ => {
                        eprintln!(
                            "\n{}{}{}",
                            "Client `".red(),
                            client_name.red(),
                            "` sent an invalid message. Disconnecting...".red(),
                        );

                        tx_disconnected.send(()).unwrap();

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
    help_msg.push_str("  exit; disconnect; quit         Gracefully disconnect all clients\n");
    help_msg.push_str("                                 and exit QuickTransfer.\n");
}
