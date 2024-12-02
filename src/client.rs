use colored::*;
use rustyline::{error::ReadlineError, history::History, DefaultEditor};
use std::fs::{self, File};
use std::net::TcpStream;
use std::path::Path;

use crate::common::messages::{
    CdAnswer, FileFail, MessageDirectoryContents, UploadResult, MESSAGE_CDANSWER, MESSAGE_DIR,
    MESSAGE_DOWNLOAD_FAIL, MESSAGE_DOWNLOAD_SUCCESS, MESSAGE_UPLOAD_RESULT,
};
use crate::common::{CommunicationAgent, ProgramOptions, ProgramRole, QuickTransferError};

pub fn handle_client(program_options: &ProgramOptions) -> Result<(), QuickTransferError> {
    println!(
        "Welcome to QuickTransfer!\nFor help, type `help`.\nConnecting to server \"{}\"...",
        program_options.server_ip_address,
    );

    let mut stream = connect_to_server(program_options)?;
    let mut agent = CommunicationAgent::new(&mut stream, ProgramRole::Client);

    agent.send_init_message()?;
    agent.receive_message_header_check(MESSAGE_DIR)?;

    println!(
        "{}",
        format!(
            "Successfully connected to {}!",
            program_options.server_ip_address.on_green().white()
        )
        .green()
        .bold()
    );

    let dir_description = agent.receive_directory_description()?;
    print_directory_contents(&dir_description);

    let mut rl = DefaultEditor::new().map_err(|_| QuickTransferError::StdinError)?;
    loop {
        let readline = rl.readline("QuickTransfer> ");
        match readline {
            Ok(ref line) => {
                rl.history_mut()
                    .add(line)
                    .map_err(|err| QuickTransferError::ReadLineError {
                        error: err.to_string(),
                    })?;
            }
            Err(ReadlineError::Interrupted) => {
                eprintln!("^C");
                return Ok(());
            }
            Err(ReadlineError::Eof) => {
                eprintln!("^D");
                return Ok(());
            }
            Err(err) => {
                return Err(QuickTransferError::ReadLineError {
                    error: err.to_string(),
                });
            }
        }

        let readline = readline.unwrap();
        let input = readline.trim();
        let mut input_splitted = input.split_whitespace();
        let command = input_splitted.next();

        let invalid_dir_name_message: &str = "`directory_name` should be either the name of a directory in current view, \".\" or \"..\".";

        match command {
            Some("cd") => {
                let directory_name = input.split_once(char::is_whitespace);
                if directory_name.is_none() {
                    eprintln!(
                        "{}",
                        format!("Usage: `cd <directory_name>`. {}", invalid_dir_name_message).red()
                    );
                    continue;
                }

                let directory_name = String::from(directory_name.unwrap().1);

                if directory_name.is_empty() {
                    eprintln!(
                        "{}",
                        format!(
                            "Error: `directory_name` cannot be empty. {}",
                            invalid_dir_name_message
                        )
                        .red()
                    );
                    continue;
                }

                agent.send_change_directory(&directory_name)?;
                agent.receive_message_header_check(MESSAGE_CDANSWER)?;

                let cd_answer = agent.receive_cd_answer()?;
                match cd_answer {
                    CdAnswer::DirectoryDoesNotExist => {
                        eprintln!(
                            "{}",
                            format!("Error: Directory `{}` does not exist!", directory_name).red()
                        );
                    }
                    CdAnswer::IllegalDirectory => {
                        eprintln!(
                            "{}",
                            format!(
                                "Error: You don't have access to directory `{}`!",
                                directory_name
                            )
                            .red()
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
                agent.send_list_directory()?;
                agent.receive_message_header_check(MESSAGE_DIR)?;
                let dir_description = agent.receive_directory_description()?;
                print_directory_contents(&dir_description);
            }
            Some("download") => {
                let file_name = parse_file_name(input, "download");
                if file_name.is_none() {
                    continue;
                }
                let file_name = file_name.unwrap();

                agent.send_download_request(&file_name)?;
                let header_received = agent.receive_message_header()?;

                match header_received.as_str() {
                    MESSAGE_DOWNLOAD_FAIL => {
                        let download_fail = agent.receive_download_fail()?;
                        match download_fail {
                            FileFail::FileDoesNotExist => {
                                eprintln!(
                                    "{}",
                                    format!("Error: File `{}` does not exist!", file_name).red()
                                );
                            }
                            FileFail::IllegalFile => {
                                eprintln!(
                                    "{}",
                                    format!(
                                        "Error: You don't have access to file `{}`!",
                                        file_name
                                    )
                                    .red()
                                );
                            }
                            FileFail::ErrorCreatingFile => {
                                eprintln!(
                                    "{}",
                                    format!("Error: Error creating file `{}`!", file_name).red()
                                );
                            }
                        }
                    }
                    MESSAGE_DOWNLOAD_SUCCESS => {
                        let file_name_truncated = file_name.split("/").last().unwrap_or(&file_name);
                        let file_size = agent.receive_message_length()?;
                        let opened_file = File::create(file_name_truncated).map_err(|_| {
                            QuickTransferError::ProblemOpeningFile {
                                file_path: String::from(file_name_truncated),
                            }
                        })?;
                        let file_path = Path::new(file_name_truncated).canonicalize().unwrap();

                        println!("Downloading file `{}`...", file_name_truncated);
                        agent.receive_file(opened_file, file_size, file_path.as_path(), false)?;
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
                if file_name.is_none() {
                    continue;
                }
                let file_name = file_name.unwrap();
                let file_path = Path::new(&file_name);

                if !fs::exists(file_path).unwrap() || !file_path.is_file() {
                    eprintln!(
                        "{}",
                        format!("Error: File `{}` does not exist!", file_name).red()
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
                agent.send_upload(opened_file, file_size, file_name_truncated, file_path)?;
                agent.receive_message_header_check(MESSAGE_UPLOAD_RESULT)?;

                let upload_result = agent.receive_upload_result()?;
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
                agent.send_disconnect_message()?;
                break;
            }
            Some("help") => {
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

                println!("  exit; disconnect; quit         Gracefully disconnect and exit QuickTransfer.\n")
            }
            Some(command) => {
                eprintln!(
                    "{}",
                    format!("Error: Command `{}` does not exist!", command).red()
                );
            }
            None => {}
        }
    }

    Ok(())
}

fn connect_to_server(program_options: &ProgramOptions) -> Result<TcpStream, QuickTransferError> {
    let stream = TcpStream::connect((
        program_options.server_ip_address.clone(),
        program_options.port,
    ))
    .map_err(|error| {
        if let Some(code) = error.raw_os_error() {
            if code == 111 {
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
        "{}",
        format!(
            "Displaying contents of {}:",
            dir_description.location().on_magenta().white()
        )
        .magenta()
    );
    for position in dir_description.positions() {
        if position.is_directory {
            print!("{}", format!("{}  ", position.name).bright_blue());
        } else {
            print!("{}", format!("{}  ", position.name).white());
        }
    }
    if dir_description.positions().is_empty() {
        print!("(empty)");
    }
    println!();
}

fn parse_file_name(input: &str, command: &str) -> Option<String> {
    let invalid_file_name_message: &'static str =
        "`file_path` should be either the path of a file relative to current view.";

    let file_name = input.split_once(char::is_whitespace);
    if file_name.is_none() {
        println!(
            "{}",
            format!(
                "Usage: `{} <file_path>`. {}",
                command, invalid_file_name_message
            )
            .red()
        );

        return None;
    }

    let file_name = String::from(file_name.unwrap().1);

    if file_name.is_empty() {
        println!(
            "{}",
            format!(
                "Note: `file_name` cannot be empty. {}",
                invalid_file_name_message
            )
            .red()
        );

        return None;
    }

    Some(file_name)
}
