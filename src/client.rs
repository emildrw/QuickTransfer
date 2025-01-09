use aes_gcm::{aead::KeyInit, Aes256Gcm, Key};
use colored::*;
use rustyline_async::{Readline, ReadlineEvent, SharedWriter};
use std::{
    fs::{self, File},
    io::{ErrorKind, Write},
    path::{Path, PathBuf}, str::SplitWhitespace,
};
use tokio::net::TcpStream;

use crate::common::{
    messages::{
        CdAnswer, DirectoryContents, FileFail, MessageDirectoryContents, MkdirAnswer, RemoveAnswer,
        RenameAnswer, UploadResult, MESSAGE_CDANSWER, MESSAGE_DIR, MESSAGE_DISCONNECT,
        MESSAGE_DOWNLOAD_FAIL, MESSAGE_DOWNLOAD_SUCCESS, MESSAGE_INIT, MESSAGE_INIT_ENC,
        MESSAGE_MKDIRANS, MESSAGE_NOT_ENC, MESSAGE_OK, MESSAGE_REMOVE_ANSWER,
        MESSAGE_RENAME_ANSWER, MESSAGE_UPLOAD_RESULT,
    },
    CommunicationAgent, ProgramOptions, ProgramRole, QuickTransferError, QuickTransferStream,
};

const INVALID_DIR_NAME_MESSAGE: &str = "`directory_name` should be either the name of a directory in current view, \".\" or \"..\".";

// This function is a wrapper to catch errors and (try to) gracefully end a connection in all cases.
pub async fn handle_client(program_options: &ProgramOptions) -> Result<(), QuickTransferError> {
    println!(
        "Welcome to QuickTransfer!\nFor help, type `help`.\nConnecting to server \"{}\" on port {}...",
        program_options.server_ip_address, program_options.port
    );

    let stream = connect_to_server(program_options).await?;

    let mut stream = if let Some(key) = &program_options.aes_key {
        let key: &Key<Aes256Gcm> = key.into();
        let cipher = Aes256Gcm::new(key);
        QuickTransferStream::new_encrypted(
            stream,
            cipher,
            ProgramRole::Client,
            program_options.timeout,
        )
    } else {
        QuickTransferStream::new_unencrypted(stream, ProgramRole::Client, program_options.timeout)
    };

    let mut agent =
        CommunicationAgent::new(&mut stream, ProgramRole::Client, program_options.timeout);
    let result = serve_client(program_options, &mut agent).await;
    if let Ok(client_disconnected) = result {
        if client_disconnected {
            let _ = agent.send_disconnect_message().await;
        }
    }

    result.map(|_| ())
}

/// This functions server program run in client mode. Returns whether the client has disconnected.
async fn serve_client(
    program_options: &ProgramOptions,
    agent: &mut CommunicationAgent<'_>,
) -> Result<bool, QuickTransferError> {
    agent
        .send_bare_message(if program_options.aes_key.is_some() {
            MESSAGE_INIT_ENC
        } else {
            MESSAGE_INIT
        })
        .await?;

    match agent.receive_bare_message_header().await?.as_str() {
        MESSAGE_NOT_ENC => {
            return Err(QuickTransferError::ServerDoesNotSupportEncryption);
        }
        MESSAGE_OK => {}
        _ => {
            return Err(QuickTransferError::SentInvalidData(ProgramRole::Client));
        }
    }

    println!(
        "{}{}{}{}{}",
        "Successfully connected to ".green().bold(),
        format!(
            "[{}]:{}",
            program_options.server_ip_address, program_options.port
        )
        .on_green()
        .white(),
        "! (connection ".green().bold(),
        if program_options.aes_key.is_some() {
            "encrypted"
        } else {
            "not encrypted"
        }
        .green()
        .bold(),
        ")".green().bold(),
    );

    let rl = Readline::new(String::from("QuickTransfer> ")).unwrap();
    let mut writer = rl.1;
    let mut rl = rl.0;
    let message_received = agent.receive_tcp(false).await?;
    let message_received = agent.read_message_header_check(&message_received, MESSAGE_DIR)?;
    if let Ok(dir_description) = agent.read_answer::<MessageDirectoryContents>(message_received) {
        if let MessageDirectoryContents::Success(dir_description) = &dir_description {
            print_directory_contents(dir_description, &mut writer)?;
        } else {
            writeln!(
                writer,
                "{}{}{}",
                "Error: An error in reading contents of `".red(),
                program_options.root_directory.clone().red(),
                "` occurred.".red(),
            )
            .map_err(|_| QuickTransferError::Stdout)?;
            writer.flush().map_err(|_| QuickTransferError::Stdout)?;

            return Err(QuickTransferError::Other);
        }
    } else {
        writeln!(
            writer,
            "{}{}{}",
            "Error: An error in reading contents of `".red(),
            program_options.root_directory.clone().red(),
            "` occurred.".red(),
        )
        .map_err(|_| QuickTransferError::Stdout)?;
        writer.flush().map_err(|_| QuickTransferError::Stdout)?;

        return Err(QuickTransferError::Other);
    }

    // Pre-print user help:
    let mut user_help = String::new();
    preprint_user_help(&mut user_help);

    loop {
        tokio::select! {
            header_received = agent.receive_message_header() => {
                let header_received = header_received?;
                if header_received.as_str() == MESSAGE_DISCONNECT {
                    println!(
                        "\n{}",
                        "Server has disconnected!".green().bold(),
                    );

                    return Ok(false);
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
                        return Ok(true);
                    }
                    Ok(ReadlineEvent::Interrupted) => {
                        eprintln!("^C");
                        return Ok(true);
                    }
                    Ok(ReadlineEvent::Line(ref line)) => {
                        rl.add_history_entry(line.to_string());

                        let input = line.trim();
                        let mut input_splitted = input.split_whitespace();
                        let command = input_splitted.next();

                        match command {
                            Some("cd") => {
                                serve_cd_command(input, &mut writer, agent).await?;
                            }
                            Some("ls") => {
                                serve_ls_command(&mut input_splitted, &mut writer, agent).await?;
                            }
                            Some("download") => {
                                serve_download_command(input, &mut writer, agent, &mut rl).await?;
                            }
                            Some("upload") => {
                                serve_upload_command(input, &mut writer, agent, &mut rl).await?;
                            }
                            Some("mkdir") => {
                                serve_mkdir_command(input, &mut writer, agent).await?;
                            }
                            Some("mv") => {
                                serve_mv_command(input, &mut writer, agent).await?;
                            }
                            Some("rm") => {
                                serve_rm_command(input, &mut writer, agent).await?;
                            }
                            Some("clear") => {
                                rl.clear().map_err(|_| QuickTransferError::Stdout)?;
                            }
                            Some("exit") | Some("disconnect") | Some("quit") => {
                                return Ok(true);
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
                            None => {}
                        }
                    }
                }
            }
        }
    }
}

/// Serves a `cd` command typed by user.
async fn serve_cd_command(input: &str, writer: &mut SharedWriter, agent: &mut CommunicationAgent<'_>) -> Result<(), QuickTransferError> {
    let directory_name = input.split_once(char::is_whitespace);
    if directory_name.is_none() {
        writeln!(
            writer,
            "{}{}",
            "Usage: `cd <directory_name>`. ".red(),
            INVALID_DIR_NAME_MESSAGE.red(),
        ).map_err(|_| QuickTransferError::Stdout)?;

        return Ok(());
    }

    let directory_name = String::from(directory_name.unwrap().1);

    if directory_name.is_empty() {
        writeln!(
            writer,
            "{}{}",
            "Error: `directory_name` cannot be empty. ".red(),
            INVALID_DIR_NAME_MESSAGE.red(),
        ).map_err(|_| QuickTransferError::Stdout)?;

        return Ok(());
    }

    agent.send_change_directory(&directory_name).await?;

    let message = agent.receive_tcp(false).await?;
    let message = agent.read_message_header_check(&message, MESSAGE_CDANSWER)?;
    let cd_answer = agent.read_answer(message)?;

    match cd_answer {
        CdAnswer::DirectoryDoesNotExist => {
            writeln!(
                writer,
                "{}{}{}",
                "Error: Directory `".red(),
                directory_name.red(),
                "` does not exist!".red(),
            ).map_err(|_| QuickTransferError::Stdout)?;
        }
        CdAnswer::ReadingDirectoryError => {
            writeln!(
                writer,
                "{}{}{}",
                "Error: An error in reading contents of `".red(),
                directory_name.red(),
                "` occurred.".red(),
            ).map_err(|_| QuickTransferError::Stdout)?;
        }
        CdAnswer::IllegalDirectory => {
            writeln!(
                writer,
                "{}{}{}",
                "Error: You don't have access to directory `".red(),
                directory_name.red(),
                "`!".red(),
            ).map_err(|_| QuickTransferError::Stdout)?;
        }
        CdAnswer::Success(dir_description) => {
            if let MessageDirectoryContents::Success(dir_description) = dir_description {
                print_directory_contents(&dir_description, writer)?;
            }
        }
    }

    Ok(())
}

/// Serves a `ls` command typed by user.
async fn serve_ls_command(input_splitted: &mut SplitWhitespace<'_>, writer: &mut SharedWriter, agent: &mut CommunicationAgent<'_>) -> Result<(), QuickTransferError> {
    if input_splitted.next().is_some() {
        writeln!(writer, "{}", "Usage: `ls`".to_string().red()).map_err(|_| QuickTransferError::Stdout)?;

        return Ok(());
    }
    agent.send_list_directory().await?;

    let message = agent.receive_tcp(false).await?;
    let message = agent.read_message_header_check(&message, MESSAGE_DIR)?;
    let dir_description = agent.read_answer(message)?;

    if let MessageDirectoryContents::Success(dir_description) = dir_description {
        print_directory_contents(&dir_description, writer)?;
    } else {
        writeln!(
            writer,
            "{}",
            "Error: An error in reading contents of the directory occurred.".red(),
        ).map_err(|_| QuickTransferError::Stdout)?;
    }

    Ok(())
}

/// Serves a `download` command typed by user.
async fn serve_download_command(input: &str, writer: &mut SharedWriter, agent: &mut CommunicationAgent<'_>, rl: &mut Readline) -> Result<(), QuickTransferError> {
    let file_name = parse_file_name(input, "download <file_path>", "<file_path>", writer);
    let Some(file_name) = file_name else {
        return Ok(());
    };

    agent.send_download_request(&file_name).await?;
    let message = agent.receive_tcp(false).await?;
    let (header_received, message) = agent.read_message_header(&message)?;

    match header_received.as_str() {
        MESSAGE_DOWNLOAD_FAIL => {
            let download_fail = agent.read_answer(message)?;
            match download_fail {
                FileFail::FileDoesNotExist => {
                    writeln!(
                        writer,
                        "{}{}{}",
                        "Error: File `".red(),
                        file_name.red(),
                        "` does not exist!".red(),
                    ).map_err(|_| QuickTransferError::Stdout)?;
                }
                FileFail::IllegalFile => {
                    writeln!(
                        writer,
                        "{}{}{}",
                        "Error: You don't have access to file `".red(),
                        file_name.red(),
                        "`!".red(),
                    ).map_err(|_| QuickTransferError::Stdout)?;
                }
                FileFail::ErrorOpeningFile => {
                    writeln!(
                        writer,
                        "{}{}{}",
                        "Error: Error opening file `".red(),
                        file_name.red(),
                        "`!".red(),
                    ).map_err(|_| QuickTransferError::Stdout)?;
                }
                FileFail::ErrorCreatingFile => {
                    writeln!(
                        writer,
                        "{}{}{}",
                        "Error: Error creating file `".red(),
                        file_name.red(),
                        "`!".red(),
                    ).map_err(|_| QuickTransferError::Stdout)?;
                }
            }
        }
        MESSAGE_DOWNLOAD_SUCCESS => {
            let file_name_truncated = Path::new(&file_name).file_name().map(|string| string.to_str().map(|string| string.to_string())).unwrap_or(Some(file_name.clone())).unwrap_or(file_name.clone());
            let (file_size, _) = agent.read_u64(message)?;
            let mut file_path_to_save = PathBuf::from("./");
            file_path_to_save.push(&file_name_truncated);
            let opened_file = File::create(&file_path_to_save).map_err(|_| {
                QuickTransferError::OpeningFile {
                    file_path: String::from(&file_name_truncated),
                }
            })?;
            let file_path = Path::new(&file_name_truncated).canonicalize().unwrap();

            writeln!(writer, "Downloading file `{}`...", file_name_truncated).map_err(|_| QuickTransferError::Stdout)?;
            rl.flush().map_err(|_| QuickTransferError::Stdout)?;
            if let Err(error) = agent.receive_file(opened_file, file_size, file_path.as_path(), false).await {
                if let QuickTransferError::WritingFile{file_path} = error {
                    writeln!(
                        writer,
                        "{}{}{}",
                        "Error: Error saving file `".red(),
                        file_path.red(),
                        "`.".red(),
                    ).map_err(|_| QuickTransferError::Stdout)?;
                    
                    return Ok(());
                } else {
                    return Err(error);
                }
            }
            writeln!(writer, "Successfully downloaded file `{}`!", file_name_truncated).map_err(|_| QuickTransferError::Stdout)?;
        }
        &_ => {
            return Err(QuickTransferError::SentInvalidData(
                ProgramRole::Client,
            ));
        }
    }

    Ok(())
}

/// Serves an `upload` command typed by user.
async fn serve_upload_command(input: &str, writer: &mut SharedWriter, agent: &mut CommunicationAgent<'_>, rl: &mut Readline) -> Result<(), QuickTransferError> {
    let file_name = parse_file_name(input, "upload <file_path>", "<file_path>",writer);
    let Some(file_name) = file_name else {
        return Ok(());
    };
    let file_path = Path::new(&file_name);

    if !fs::exists(file_path).unwrap() || !file_path.is_file() {
        writeln!(
            writer,
            "{}{}{}",
            "Error: File `".red(),
            file_name.red(),
            "` does not exist!".red(),
        ).map_err(|_| QuickTransferError::Stdout)?;

        return Ok(())
    }

    let Ok(opened_file) = File::open(file_path) else {
        writeln!(
            writer,
            "{}{}{}",
            "Error opening file `".red(),
            file_name.red(),
            "`.".red(),
        ).map_err(|_| QuickTransferError::Stdout)?;

        return Ok(());
    };

    let Ok(file_size) = opened_file.metadata() else {
        writeln!(
            writer,
            "{}{}{}",
            "Error reading size of file `".red(),
            file_name.red(),
            "`.".red(),
        ).map_err(|_| QuickTransferError::Stdout)?;

        return Ok(());
    };
    let file_name_truncated = Path::new(&file_name).file_name().map(|string| string.to_str().map(|string| string.to_string())).unwrap_or(Some(file_name.clone())).unwrap_or(file_name.clone());

    writeln!(writer, "Uploading file `{}`...", file_name).map_err(|_| QuickTransferError::Stdout)?;
    rl.flush().map_err(|_| QuickTransferError::Stdout)?;
    agent.send_upload(opened_file, file_size.len(), &file_name_truncated, file_path).await?;

    let message = agent.receive_tcp(false).await?;
    let message = agent.read_message_header_check(&message, MESSAGE_UPLOAD_RESULT)?;
    let upload_result = agent.read_answer(message)?;

    match upload_result {
        UploadResult::Fail(fail) => match fail {
            FileFail::ErrorCreatingFile => {
                writeln!(writer, "Uploading file `{}` failed. An error creating the file on server occurred.", file_name).map_err(|_| QuickTransferError::Stdout)?;
            }
            _ => {
                writeln!(writer, "Uploading file `{}` failed.", file_name).map_err(|_| QuickTransferError::Stdout)?;
            }
        },
        UploadResult::Success => {
            writeln!(writer, "Successfully uploaded file `{}`!", file_name).map_err(|_| QuickTransferError::Stdout)?;
        }
    }

    Ok(())
}

/// Serves a `mkdir` command typed by user.
async fn serve_mkdir_command(input: &str, writer: &mut SharedWriter, agent: &mut CommunicationAgent<'_>) -> Result<(), QuickTransferError> {
    let directory_name = input.split_once(char::is_whitespace);
    if directory_name.is_none() {
        writeln!(
            writer,
            "{}{}",
            "Usage: `mkdir <directory_name>`. ".red(),
            INVALID_DIR_NAME_MESSAGE.red(),
        ).map_err(|_| QuickTransferError::Stdout)?;

        return Ok(());
    }

    let directory_name = String::from(directory_name.unwrap().1);

    if directory_name.is_empty() {
        writeln!(
            writer,
            "{}{}",
            "Error: `directory_name` cannot be empty. ".red(),
            INVALID_DIR_NAME_MESSAGE.red(),
        ).map_err(|_| QuickTransferError::Stdout)?;
        
        return Ok(());
    }

    agent.send_mkdir(&directory_name).await?;

    let message = agent.receive_tcp(false).await?;
    let message = agent.read_message_header_check(&message, MESSAGE_MKDIRANS)?;
    let mkdir_answer = agent.read_answer(message)?;

    match mkdir_answer {
        MkdirAnswer::DirectoryAlreadyExists => {
            writeln!(
                writer,
                "{}{}{}",
                "Error: Directory `".red(),
                directory_name.red(),
                "` already exists!".red(),
            ).map_err(|_| QuickTransferError::Stdout)?;

        }
        MkdirAnswer::IllegalDirectory => {
            writeln!(
                writer,
                "{}{}{}",
                "Error: You don't have access to directory `".red(),
                directory_name.red(),
                "`!".red(),
            ).map_err(|_| QuickTransferError::Stdout)?;
        }
        MkdirAnswer::ErrorCreatingDirectory => {
            writeln!(
                writer,
                "{}{}{}",
                "Error: An error creating directory `".red(),
                directory_name.red(),
                "` has occurred.".red(),
            ).map_err(|_| QuickTransferError::Stdout)?;
        }
        MkdirAnswer::Success => {
            writeln!(
                writer,
                "Successfully created directory `{}`.",
                directory_name
            ).map_err(|_| QuickTransferError::Stdout)?;

            agent.send_list_directory().await?;

            let message = agent.receive_tcp(false).await?;
            let message = agent.read_message_header_check(&message, MESSAGE_DIR)?;
            let dir_description = agent.read_answer(message)?;

            if let MessageDirectoryContents::Success(dir_description) = dir_description {
                print_directory_contents(&dir_description, writer)?;
            }
        }
    }

    Ok(())
}

/// Serves a `mv` command typed by user.
async fn serve_mv_command(input: &str, writer: &mut SharedWriter, agent: &mut CommunicationAgent<'_>) -> Result<(), QuickTransferError> {
    let Some((file_dir_name, new_name)) = parse_file_dir_name_and_name(input, "mv <file_dir_path> <new_name>", writer) else {
        return Ok(())
    };

    agent.send_rename_request(&file_dir_name, &new_name).await?;

    let message = agent.receive_tcp(false).await?;
    let message = agent.read_message_header_check(&message, MESSAGE_RENAME_ANSWER)?;
    let rename_answer = agent.read_answer(message)?;

    match rename_answer {
        RenameAnswer::FileDirDoesNotExist => {
            writeln!(
                writer,
                "{}{}{}",
                "Error: File/directory `".red(),
                file_dir_name.red(),
                "` does not exist!".red(),
            ).map_err(|_| QuickTransferError::Stdout)?;
        }
        RenameAnswer::IllegalFileDir => {
            writeln!(
                writer,
                "{}{}{}",
                "Error: You don't have access to file/directory `".red(),
                file_dir_name.red(),
                "`!".red(),
            ).map_err(|_| QuickTransferError::Stdout)?;
        }
        RenameAnswer::ErrorRenaming => {
            writeln!(
                writer,
                "{}{}{}",
                "Error: An error renaming file/directory `".red(),
                file_dir_name.red(),
                "` has occurred.".red(),
            ).map_err(|_| QuickTransferError::Stdout)?;
        }
        RenameAnswer::Success => {
            writeln!(
                writer,
                "Successfully renamed `{}` to `{}`.",
                file_dir_name,
                new_name
            ).map_err(|_| QuickTransferError::Stdout)?;

            agent.send_list_directory().await?;

            let message = agent.receive_tcp(false).await?;
            let message = agent.read_message_header_check(&message, MESSAGE_DIR)?;
            let dir_description = agent.read_answer(message)?;

            if let MessageDirectoryContents::Success(dir_description) = dir_description {
                print_directory_contents(&dir_description, writer)?;
            }
        }
    }

    Ok(())
}

/// Serves a `rm` command typed by user.
async fn serve_rm_command(input: &str, writer: &mut SharedWriter, agent: &mut CommunicationAgent<'_>) -> Result<(), QuickTransferError> {
    let file_dir_name = parse_file_name(input, "rm <file_dir_path>", "<file_dir_path>", writer);
    let Some(file_dir_name) = file_dir_name else {
        return Ok(())
    };

    agent.send_remove_request(&file_dir_name).await?;

    let message = agent.receive_tcp(false).await?;
    let message = agent.read_message_header_check(&message, MESSAGE_REMOVE_ANSWER)?;
    let remove_answer = agent.read_answer(message)?;

    match remove_answer {
        RemoveAnswer::FileDirDoesNotExist => {
            writeln!(
                writer,
                "{}{}{}",
                "Error: File/directory `".red(),
                file_dir_name.red(),
                "` does not exist!".red(),
            ).map_err(|_| QuickTransferError::Stdout)?;
        }
        RemoveAnswer::IllegalFileDir => {
            writeln!(
                writer,
                "{}{}{}",
                "Error: You don't have access to file/directory `".red(),
                file_dir_name.red(),
                "`!".red(),
            ).map_err(|_| QuickTransferError::Stdout)?;
        }
        RemoveAnswer::ErrorRemoving => {
            writeln!(
                writer,
                "{}{}{}",
                "Error: An error removing file/directory `".red(),
                file_dir_name.red(),
                "` has occurred.".red(),
            ).map_err(|_| QuickTransferError::Stdout)?;
        }
        RemoveAnswer::DirectoryNotEmpty => {
            writeln!(
                writer,
                "{}{}{}",
                "Error: Directory `".red(),
                file_dir_name.red(),
                "` is not empty! For safety reasons, deleting non-empty directories is not allowed.".red(),
            ).map_err(|_| QuickTransferError::Stdout)?;
        }
        RemoveAnswer::Success => {
            writeln!(
                writer,
                "Successfully removed `{}`.",
                file_dir_name,
            ).map_err(|_| QuickTransferError::Stdout)?;

            agent.send_list_directory().await?;

            let message = agent.receive_tcp(false).await?;
            let message = agent.read_message_header_check(&message, MESSAGE_DIR)?;
            let dir_description = agent.read_answer(message)?;

            if let MessageDirectoryContents::Success(dir_description) = dir_description {
                print_directory_contents(&dir_description, writer)?;
            }
        }
    }

    Ok(())
}

/// Connects client to a server.
async fn connect_to_server(
    program_options: &ProgramOptions,
) -> Result<TcpStream, QuickTransferError> {
    let stream = TcpStream::connect((
        program_options.server_ip_address.clone(),
        program_options.port,
    ))
    .await
    .map_err(|error| {
        if error.kind() == ErrorKind::ConnectionRefused {
            return QuickTransferError::ConnectionRefused {
                server_ip: program_options.server_ip_address.clone(),
                port: program_options.port,
            };
        }

        QuickTransferError::ConnectionCreation
    })?;

    Ok(stream)
}

fn print_directory_contents(
    dir_description: &DirectoryContents,
    writer: &mut SharedWriter,
) -> Result<(), QuickTransferError> {
    writeln!(
        writer,
        "{}{}{}",
        "Displaying contents of ".magenta(),
        dir_description.location.on_magenta().white(),
        ":".magenta()
    )
    .map_err(|_| QuickTransferError::Stdout)?;

    for position in &dir_description.positions {
        if position.is_directory {
            write!(writer, "{}    ", position.name.bright_blue())
                .map_err(|_| QuickTransferError::Stdout)?;
        } else {
            write!(writer, "{}    ", position.name.white())
                .map_err(|_| QuickTransferError::Stdout)?;
        }
    }
    if dir_description.positions.is_empty() {
        write!(writer, "(empty)").map_err(|_| QuickTransferError::Stdout)?;
    }
    writeln!(writer).map_err(|_| QuickTransferError::Stdout)?;

    Ok(())
}

/// Parses file name returning error, if needed.
fn parse_file_name(
    input: &str,
    command: &str,
    file_path_name: &str,
    writer: &mut SharedWriter,
) -> Option<String> {
    let file_name = input.split_once(char::is_whitespace);
    if file_name.is_none() {
        let _ = writeln!(
            writer,
            "{}{}{}{}{}",
            "Usage: `".red(),
            command.red(),
            "`. ".red(),
            file_path_name.red(),
            "` should be either the path of a file relative to current view.".red()
        );

        return None;
    }

    let file_name = String::from(file_name.unwrap().1);

    if file_name.is_empty() {
        let _ = writeln!(
            writer,
            "{}{}{}",
            "Note: `".red(),
            file_path_name.red(),
            "` cannot be empty. ".red()
        );

        return None;
    }

    Some(file_name)
}

/// Parses file name and second argument returning error, if needed.
fn parse_file_dir_name_and_name(
    input: &str,
    command: &str,
    writer: &mut SharedWriter,
) -> Option<(String, String)> {
    let mut file_name = input.splitn(3, char::is_whitespace);
    if file_name.next().is_none() {
        let _ = writeln!(writer, "{}{}", "Usage: `".red(), command.red());

        return None;
    }

    let file_name1 = String::from(file_name.next().unwrap_or(""));
    let file_name2 = String::from(file_name.next().unwrap_or(""));

    if file_name1.is_empty() {
        let _ = writeln!(
            writer,
            "{}",
            "Note: `file_dir_path` should be either the path of a file relative to current view.. "
                .red(),
        );

        return None;
    }

    if file_name2.is_empty() {
        let _ = writeln!(writer, "{}", "Note: `new_name` cannot be empty. ".red());

        return None;
    }

    Some((file_name1, file_name2))
}

/// Pre-prints user help so as not to do it every time.
fn preprint_user_help(help_msg: &mut String) {
    help_msg.push_str("Available commands:\n");
    help_msg.push_str("  cd <directory_name>            Change directory to `directory_name`\n");
    help_msg.push_str("                                 (can be a path, including `..`; note:\n");
    help_msg.push_str("                                 you cannot go higher that the root\n");
    help_msg.push_str(
        "                                 directory in which the server is being run).\n",
    );

    help_msg.push_str("  ls                             Display current directory contents.\n");

    help_msg.push_str("  download <file_path>           Download the file from `file_path`\n");
    help_msg.push_str("                                 (relative to current view) to current\n");
    help_msg.push_str("                                 directory (i.e. on which QuickTransfer\n");
    help_msg.push_str("                                 has been run). If the file exists, it\n");
    help_msg.push_str("                                 will be overwritten.\n");

    help_msg
        .push_str("  upload <file_path>             Upload the file from `file_path` (relative\n");
    help_msg.push_str("                                 to current directory, i.e. on which\n");
    help_msg
        .push_str("                                 QuickTransfer has been run) to directory\n");
    help_msg.push_str("                                 in current view (overrides files). If\n");
    help_msg
        .push_str("                                 the file exists, it will be overwritten.\n");
    help_msg
        .push_str("  mkdir <directory_name>         Create a new directory in current location.\n");
    help_msg.push_str("  mv <file_dir_path> <new_name>  Rename a file/directory.\n");
    help_msg.push_str("  rm <file_dir_path>             Remove a file/empty directory.\n");
    help_msg.push_str("  clear                          Clear the screen.\n");

    help_msg.push_str(
        "  exit; disconnect; quit         Gracefully disconnect and exit QuickTransfer.\n",
    );
}
