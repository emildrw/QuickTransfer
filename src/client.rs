use colored::*;
use rustyline_async::{Readline, ReadlineEvent, SharedWriter};
use std::{
    fs::{self, File},
    io::Write,
    path::Path,
};
use tokio::net::TcpStream;

use crate::common::messages::{
    CdAnswer, FileFail, MessageDirectoryContents, UploadResult, ECONNREFUSED, MESSAGE_CDANSWER,
    MESSAGE_DIR, MESSAGE_DISCONNECT, MESSAGE_DOWNLOAD_FAIL, MESSAGE_DOWNLOAD_SUCCESS,
    MESSAGE_UPLOAD_RESULT,
};
use crate::common::{CommunicationAgent, ProgramOptions, ProgramRole, QuickTransferError};

// This function is a wrapper to catch errors and (try to) gracefully end a connection in all cases.
pub async fn handle_client(program_options: &ProgramOptions) -> Result<(), QuickTransferError> {
    println!(
        "Welcome to QuickTransfer!\nFor help, type `help`.\nConnecting to server \"{}\" on port {}...",
        program_options.server_ip_address, program_options.port
    );

    let mut stream = connect_to_server(program_options).await?;
    let mut agent = CommunicationAgent::new(&mut stream, ProgramRole::Client);
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
    agent.send_init_message().await?;
    agent.receive_message_header_check(MESSAGE_DIR).await?;

    println!(
        "{}{}{}",
        "Successfully connected to ".green().bold(),
        program_options.server_ip_address.on_green().white(),
        "!".green().bold(),
    );

    let rl = Readline::new(String::from("QuickTransfer> ")).unwrap();
    let mut writer = rl.1;
    let mut rl = rl.0;

    let dir_description = agent.receive_directory_description().await?;
    print_directory_contents(&dir_description, &mut writer)?;

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
                        return Err(QuickTransferError::ReadLineError {
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

                        let invalid_dir_name_message: &str = "`directory_name` should be either the name of a directory in current view, \".\" or \"..\".";

                        match command {
                            Some("cd") => {
                                let directory_name = input.split_once(char::is_whitespace);
                                if directory_name.is_none() {
                                    writeln!(
                                        writer,
                                        "{}{}",
                                        "Usage: `cd <directory_name>`. ".red(),
                                        invalid_dir_name_message.red(),
                                    ).map_err(|_| QuickTransferError::StdoutError)?;

                                    continue;
                                }

                                let directory_name = String::from(directory_name.unwrap().1);

                                if directory_name.is_empty() {
                                    writeln!(
                                        writer,
                                        "{}{}",
                                        "Error: `directory_name` cannot be empty. ".red(),
                                        invalid_dir_name_message.red(),
                                    ).map_err(|_| QuickTransferError::StdoutError)?;
                                    continue;
                                }

                                agent.send_change_directory(&directory_name).await?;
                                agent.receive_message_header_check(MESSAGE_CDANSWER).await?;

                                let cd_answer = agent.receive_cd_answer().await?;
                                match cd_answer {
                                    CdAnswer::DirectoryDoesNotExist => {
                                        writeln!(
                                            writer,
                                            "{}{}{}",
                                            "Error: Directory `".red(),
                                            directory_name.red(),
                                            "` does not exist!".red(),
                                        ).map_err(|_| QuickTransferError::StdoutError)?;
                                    }
                                    CdAnswer::IllegalDirectory => {
                                        writeln!(
                                            writer,
                                            "{}{}{}",
                                            "Error: You don't have access to directory `".red(),
                                            directory_name.red(),
                                            "`!".red(),
                                        ).map_err(|_| QuickTransferError::StdoutError)?;
                                    }
                                    CdAnswer::Success(dir_description) => {
                                        print_directory_contents(&dir_description, &mut writer)?;
                                    }
                                }
                            }
                            Some("ls") => {
                                if input_splitted.next().is_some() {
                                    writeln!(writer, "{}", "Usage: `ls`".to_string().red()).map_err(|_| QuickTransferError::StdoutError)?;

                                    continue;
                                }
                                agent.send_list_directory().await?;
                                agent.receive_message_header_check(MESSAGE_DIR).await?;
                                let dir_description = agent.receive_directory_description().await?;
                                print_directory_contents(&dir_description, &mut writer)?;
                            }
                            Some("download") => {
                                let file_name = parse_file_name(input, "download", &mut writer);
                                let Some(file_name) = file_name else {
                                    continue;
                                };

                                agent.send_download_request(&file_name).await?;
                                let header_received = agent.receive_message_header().await?;

                                match header_received.as_str() {
                                    MESSAGE_DOWNLOAD_FAIL => {
                                        let download_fail = agent.receive_download_fail().await?;
                                        match download_fail {
                                            FileFail::FileDoesNotExist => {
                                                writeln!(
                                                    writer,
                                                    "{}{}{}",
                                                    "Error: File `".red(),
                                                    file_name.red(),
                                                    "` does not exist!".red(),
                                                ).map_err(|_| QuickTransferError::StdoutError)?;
                                            }
                                            FileFail::IllegalFile => {
                                                writeln!(
                                                    writer,
                                                    "{}{}{}",
                                                    "Error: You don't have access to file `".red(),
                                                    file_name.red(),
                                                    "{}`!".red(),
                                                ).map_err(|_| QuickTransferError::StdoutError)?;
                                            }
                                            FileFail::ErrorCreatingFile => {
                                                writeln!(
                                                    writer,
                                                    "{}{}{}",
                                                    "Error: Error creating file `".red(),
                                                    file_name.red(),
                                                    "{}`!".red(),
                                                ).map_err(|_| QuickTransferError::StdoutError)?;
                                            }
                                        }
                                    }
                                    MESSAGE_DOWNLOAD_SUCCESS => {
                                        let file_name_truncated = file_name.split("/").last().unwrap_or(&file_name);
                                        let file_size = agent.receive_message_length().await?;
                                        let opened_file = File::create(file_name_truncated).map_err(|_| {
                                            QuickTransferError::ProblemOpeningFile {
                                                file_path: String::from(file_name_truncated),
                                            }
                                        })?;
                                        let file_path = Path::new(file_name_truncated).canonicalize().unwrap();

                                        writeln!(writer, "Downloading file `{}`...", file_name_truncated).map_err(|_| QuickTransferError::StdoutError)?;
                                        agent.receive_file(opened_file, file_size, file_path.as_path(), false).await?;
                                        writeln!(writer, "Successfully downloaded file `{}`!", file_name_truncated).map_err(|_| QuickTransferError::StdoutError)?;
                                    }
                                    &_ => {
                                        return Err(QuickTransferError::SentInvalidData(
                                            program_options.program_role,
                                        ));
                                    }
                                }
                            }
                            Some("upload") => {
                                let file_name = parse_file_name(input, "upload", &mut writer);
                                let Some(file_name) = file_name else {
                                    continue;
                                };
                                let file_path = Path::new(&file_name);

                                if !fs::exists(file_path).unwrap() || !file_path.is_file() {
                                    writeln!(
                                        writer,
                                        "{}{}{}",
                                        "Error: File `".red(),
                                        file_name.red(),
                                        "` does not exist!".red(),
                                    ).map_err(|_| QuickTransferError::StdoutError)?;

                                    continue;
                                }

                                let opened_file =
                                    File::open(file_path).map_err(|_| QuickTransferError::ProblemOpeningFile {
                                        file_path: String::from(file_path.to_str().unwrap()),
                                    })?;

                                let file_size = opened_file.metadata().unwrap().len();
                                let file_name_truncated = file_name.split("/").last().unwrap_or(&file_name);

                                writeln!(writer, "Uploading file `{}`...", file_name).map_err(|_| QuickTransferError::StdoutError)?;
                                agent.send_upload(opened_file, file_size, file_name_truncated, file_path).await?;
                                agent.receive_message_header_check(MESSAGE_UPLOAD_RESULT).await?;

                                let upload_result = agent.receive_upload_result().await?;
                                match upload_result {
                                    UploadResult::Fail(fail) => match fail {
                                        FileFail::ErrorCreatingFile => {
                                            writeln!(writer, "Uploading file `{}` failed. An error creating the file on server occurred.", file_name).map_err(|_| QuickTransferError::StdoutError)?;
                                        }
                                        _ => {
                                            writeln!(writer, "Uploading file `{}` failed.", file_name).map_err(|_| QuickTransferError::StdoutError)?;
                                        }
                                    },
                                    UploadResult::Success => {
                                        writeln!(writer, "Successfully uploaded file `{}`!", file_name).map_err(|_| QuickTransferError::StdoutError)?;
                                    }
                                }
                            }
                            Some("exit") | Some("disconnect") | Some("quit") => {
                                return Ok(true);
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
                            None => {}
                        }
                    }
                }
            }
        }
    }
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
        if let Some(code) = error.raw_os_error() {
            if code == ECONNREFUSED {
                return QuickTransferError::CouldntConnectToServer {
                    server_ip: program_options.server_ip_address.clone(),
                    port: program_options.port,
                };
            }
        }
        QuickTransferError::ConnectionCreation
    })?;

    Ok(stream)
}

fn print_directory_contents(
    dir_description: &MessageDirectoryContents,
    writer: &mut SharedWriter,
) -> Result<(), QuickTransferError> {
    writeln!(
        writer,
        "{}{}{}",
        "Displaying contents of ".magenta(),
        dir_description.location().on_magenta().white(),
        ":".magenta()
    )
    .map_err(|_| QuickTransferError::StdoutError)?;

    for position in dir_description.positions() {
        if position.is_directory {
            write!(writer, "{}    ", position.name.bright_blue())
                .map_err(|_| QuickTransferError::StdoutError)?;
        } else {
            write!(writer, "{}    ", position.name.white())
                .map_err(|_| QuickTransferError::StdoutError)?;
        }
    }
    if dir_description.positions().is_empty() {
        write!(writer, "(empty)").map_err(|_| QuickTransferError::StdoutError)?;
    }
    writeln!(writer).map_err(|_| QuickTransferError::StdoutError)?;

    Ok(())
}

/// Parses file name returning error, if needed.
fn parse_file_name(input: &str, command: &str, writer: &mut SharedWriter) -> Option<String> {
    let invalid_file_name_message: &'static str =
        "`file_path` should be either the path of a file relative to current view.";

    let file_name = input.split_once(char::is_whitespace);
    if file_name.is_none() {
        let _ = writeln!(
            writer,
            "{}{}{}{}",
            "Usage: `".red(),
            command.red(),
            " <file_path>`. ".red(),
            invalid_file_name_message.red(),
        );

        return None;
    }

    let file_name = String::from(file_name.unwrap().1);

    if file_name.is_empty() {
        let _ = writeln!(
            writer,
            "{}{}",
            "Note: `file_name` cannot be empty. ".red(),
            invalid_file_name_message.red(),
        );

        return None;
    }

    Some(file_name)
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

    help_msg.push_str(
        "  exit; disconnect; quit         Gracefully disconnect and exit QuickTransfer.\n",
    );
}
