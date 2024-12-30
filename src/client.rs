use colored::*;
use rustyline::{error::ReadlineError, history::History, DefaultEditor};
use std::fs::{self, File};
use std::sync::{Arc, Mutex};
// use std::net::TcpStream;
use tokio::net::TcpStream;
use std::path::Path;

use crate::common::messages::{
    CdAnswer, FileFail, MessageDirectoryContents, UploadResult, ECONNREFUSED, MESSAGE_CDANSWER, MESSAGE_DIR, MESSAGE_DISCONNECT, MESSAGE_DOWNLOAD_FAIL, MESSAGE_DOWNLOAD_SUCCESS, MESSAGE_UPLOAD_RESULT
};
use crate::common::{CommunicationAgent, ProgramOptions, ProgramRole, QuickTransferError, ServerCommand};

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
async fn serve_client(program_options: &ProgramOptions, agent: &mut CommunicationAgent<'_>) -> Result<bool, QuickTransferError> {
    agent.send_init_message().await?;
    agent.receive_message_header_check(MESSAGE_DIR).await?;

    println!(
        "{}{}{}",
        "Successfully connected to ".green().bold(),
        program_options.server_ip_address.on_green().white(),
        "!".green().bold(),
    );

    let dir_description = agent.receive_directory_description().await?;
    print_directory_contents(&dir_description);

    let rl = DefaultEditor::new().map_err(|_| QuickTransferError::StdinError)?;
    let mut rl = Arc::new(Mutex::new(rl));

    loop {
        let result = agent.receive_stdin_or_header(Arc::clone(&mut rl)).await?;
        match result {
            ServerCommand::MessageHeader(header_received) => {
                match header_received.as_str() {
                    MESSAGE_DISCONNECT => {
                        println!(
                            "\n{}",
                            "Server disconnected!".green().bold(),
                        );
                        
                        return Ok(false);
                    }
                    _ => {

                    }
                }
            }
            ServerCommand::Stdin(command) => {
                match command {
                    Ok(ref line) => {
                        let mut rl = rl.lock().unwrap();
                        rl.history_mut()
                            .add(line)
                            .map_err(|err| QuickTransferError::ReadLineError {
                                error: err.to_string(),
                            })?;
                    }
                    Err(ReadlineError::Interrupted) => {
                        eprintln!("^C");
                        return Ok(true);
                    }
                    Err(ReadlineError::Eof) => {
                        eprintln!("^D");
                        return Ok(true);
                    }
                    Err(err) => {
                        return Err(QuickTransferError::ReadLineError {
                            error: err.to_string(),
                        });
                    }
                }

                let Ok(readline) = command else {
                    return Ok(true);
                };
                let input = readline.trim();
                let mut input_splitted = input.split_whitespace();
                let command = input_splitted.next();
        
                let invalid_dir_name_message: &str = "`directory_name` should be either the name of a directory in current view, \".\" or \"..\".";
        
                match command {
                    Some("cd") => {
                        let directory_name = input.split_once(char::is_whitespace);
                        if directory_name.is_none() {
                            eprintln!(
                                "{}{}",
                                "Usage: `cd <directory_name>`. ".red(),
                                invalid_dir_name_message.red(),
                            );
                            continue;
                        }
        
                        let directory_name = String::from(directory_name.unwrap().1);
        
                        if directory_name.is_empty() {
                            eprintln!(
                                "{}{}",
                                "Error: `directory_name` cannot be empty. ".red(),
                                invalid_dir_name_message.red(),
                            );
                            continue;
                        }
        
                        agent.send_change_directory(&directory_name).await?;
                        agent.receive_message_header_check(MESSAGE_CDANSWER).await?;
        
                        let cd_answer = agent.receive_cd_answer().await?;
                        match cd_answer {
                            CdAnswer::DirectoryDoesNotExist => {
                                eprintln!(
                                    "{}{}{}",
                                    "Error: Directory `".red(),
                                    directory_name.red(),
                                    "` does not exist!".red(),
                                );
                            }
                            CdAnswer::IllegalDirectory => {
                                eprintln!(
                                    "{}{}{}",
                                    "Error: You don't have access to directory `".red(),
                                    directory_name.red(),
                                    "`!".red(),
                                );
                            }
                            CdAnswer::Success(dir_description) => {
                                print_directory_contents(&dir_description);
                            }
                        }
                    }
                    Some("ls") => {
                        if input_splitted.next().is_some() {
                            eprintln!("{}", "Usage: `ls`".to_string().red());
                            continue;
                        }
                        agent.send_list_directory().await?;
                        agent.receive_message_header_check(MESSAGE_DIR).await?;
                        let dir_description = agent.receive_directory_description().await?;
                        print_directory_contents(&dir_description);
                    }
                    Some("download") => {
                        let file_name = parse_file_name(input, "download");
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
                                        eprintln!(
                                            "{}{}{}",
                                            "Error: File `".red(),
                                            file_name.red(),
                                            "` does not exist!".red(),
                                        );
                                    }
                                    FileFail::IllegalFile => {
                                        eprintln!(
                                            "{}{}{}",
                                            "Error: You don't have access to file `".red(),
                                            file_name.red(),
                                            "{}`!".red(),
                                        );
                                    }
                                    FileFail::ErrorCreatingFile => {
                                        eprintln!(
                                            "{}{}{}",
                                            "Error: Error creating file `".red(),
                                            file_name.red(),
                                            "{}`!".red(),
                                        );
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
        
                                println!("Downloading file `{}`...", file_name_truncated);
                                agent.receive_file(opened_file, file_size, file_path.as_path(), false).await?;
                                println!("Successfully downloaded file `{}`!", file_name_truncated);
                            }
                            &_ => {
                                return Err(QuickTransferError::SentInvalidData(
                                    program_options.program_role,
                                ));
                            }
                        }
                    }
                    Some("upload") => {
                        let file_name = parse_file_name(input, "upload");
                        let Some(file_name) = file_name else {
                            continue;
                        };
                        let file_path = Path::new(&file_name);
        
                        if !fs::exists(file_path).unwrap() || !file_path.is_file() {
                            eprintln!(
                                "{}{}{}",
                                "Error: File `".red(),
                                file_name.red(),
                                "` does not exist!".red(),
                            );
                            continue;
                        }
        
                        let opened_file =
                            File::open(file_path).map_err(|_| QuickTransferError::ProblemOpeningFile {
                                file_path: String::from(file_path.to_str().unwrap()),
                            })?;
        
                        let file_size = opened_file.metadata().unwrap().len();
                        let file_name_truncated = file_name.split("/").last().unwrap_or(&file_name);
        
                        println!("Uploading file `{}`...", file_name);
                        agent.send_upload(opened_file, file_size, file_name_truncated, file_path).await?;
                        agent.receive_message_header_check(MESSAGE_UPLOAD_RESULT).await?;
        
                        let upload_result = agent.receive_upload_result().await?;
                        match upload_result {
                            UploadResult::Fail(fail) => match fail {
                                FileFail::ErrorCreatingFile => {
                                    println!("Uploading file `{}` failed. An error creating the file on server occurred.", file_name);
                                }
                                _ => {
                                    println!("Uploading file `{}` failed.", file_name);
                                }
                            },
                            UploadResult::Success => {
                                println!("Successfully uploaded file `{}`!", file_name);
                            }
                        }
                    }
                    Some("exit") | Some("disconnect") | Some("quit") => {
                        break;
                    }
                    Some("help") => {
                        print_user_help();
                    }
                    Some(command) => {
                        eprintln!(
                            "{}{}{}",
                            "Error: Command `".red(),
                            command.red(),
                            "` does not exist!".red(),
                        );
                    }
                    None => {}
                }
            }
        }
    }

    Ok(true)
}

/// Connects client to a server.
async fn connect_to_server(program_options: &ProgramOptions) -> Result<TcpStream, QuickTransferError> {
    let stream = TcpStream::connect((
        program_options.server_ip_address.clone(),
        program_options.port,
    ))
    .await.map_err(|error| {
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

fn print_directory_contents(dir_description: &MessageDirectoryContents) {
    println!(
        "{}{}{}",
        "Displaying contents of ".magenta(),
        dir_description.location().on_magenta().white(),
        ":".magenta()
    );
    for position in dir_description.positions() {
        if position.is_directory {
            print!("{}    ", position.name.bright_blue());
        } else {
            print!("{}    ", position.name.white());
        }
    }
    if dir_description.positions().is_empty() {
        print!("(empty)");
    }
    println!();
}

/// Parses file name returning error, if needed.
fn parse_file_name(input: &str, command: &str) -> Option<String> {
    let invalid_file_name_message: &'static str =
        "`file_path` should be either the path of a file relative to current view.";

    let file_name = input.split_once(char::is_whitespace);
    if file_name.is_none() {
        println!(
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
        println!(
            "{}{}",
            "Note: `file_name` cannot be empty. ".red(),
            invalid_file_name_message.red(),
        );

        return None;
    }

    Some(file_name)
}

fn print_user_help() {
    println!("Available commands:");
    println!("  cd <directory_name>            Change directory to `directory_name`");
    println!("                                 (can be a path, including `..`; note:");
    println!("                                 you cannot go higher that the root");
    println!(
        "                                 directory in which the server is being run)."
    );

    println!("  ls                             Display current directory contents.");

    println!("  download <file_path>           Download the file from `file_path`");
    println!("                                 (relative to current view) to current");
    println!("                                 directory (i.e. on which QuickTransfer");
    println!("                                 has been run). If the file exists, it");
    println!("                                 will be overwritten.");

    println!(
        "  upload <file_path>             Upload the file from `file_path` (relative"
    );
    println!("                                 to current directory, i.e. on which");
    println!(
        "                                 QuickTransfer has been run) to directory"
    );
    println!("                                 in current view (overrides files). If");
    println!(
        "                                 the file exists, it will be overwritten."
    );

    println!("  exit; disconnect; quit         Gracefully disconnect and exit QuickTransfer.\n");
}
