use colored::*;
use std::fs::{self, File};
use std::io::{self, Write};
use std::net::TcpStream;
use std::path::Path;

use crate::common::messages::{
    CdAnswer, FileFail, MessageDirectoryContents, UploadResult, MESSAGE_CDANSWER, MESSAGE_DIR,
    MESSAGE_DOWNLOAD_FAIL, MESSAGE_DOWNLOAD_SUCCESS, MESSAGE_UPLOAD_RESULT,
};
use crate::common::{CommunicationAgent, ProgramOptions, ProgramRole, QuickTransferError};

pub fn handle_client(program_options: ProgramOptions) -> Result<(), QuickTransferError> {
    println!(
        "Welcome to QuickTransfer!\nFor help, type `help`.\nConnecting to server \"{}\"...",
        program_options.server_ip_address,
    );

    let mut stream = connect_to_server(&program_options)?;
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

    loop {
        print!("QuickTransfer> ");
        io::stdout()
            .flush()
            .map_err(|_| QuickTransferError::StdoutError)?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|_| QuickTransferError::StdinError)?;
        let input = input.trim();
        let command = input.split_whitespace().next();

        let invalid_dir_name_message: &str = "`directory_name` should be either the name of a directory in current view, \".\" or \"..\".";

        match command {
            Some("cd") => {
                let directory_name = input.split_once(char::is_whitespace);
                if directory_name.is_none() {
                    println!(
                        "{}",
                        format!("Usage: `cd <directory_name>`. {}", invalid_dir_name_message).red()
                    );
                    continue;
                }

                let directory_name = String::from(directory_name.unwrap().1);

                if directory_name.is_empty() {
                    println!(
                        "{}",
                        format!(
                            "Note: `directory_name` cannot be empty. {}",
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
                        println!(
                            "{}",
                            format!("Error: Directory `{}` does not exist!", directory_name).red()
                        );
                    }
                    CdAnswer::IllegalDirectory => {
                        println!(
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
                                println!(
                                    "{}",
                                    format!("Error: File `{}` does not exist!", file_name).red()
                                );
                            }
                            FileFail::IllegalFile => {
                                println!(
                                    "{}",
                                    format!(
                                        "Error: You don't have access to file `{}`!",
                                        file_name
                                    )
                                    .red()
                                );
                            }
                            FileFail::ErrorCreatingFile => {
                                println!(
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
                    println!(
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
            Some("help") => {}
            Some(&_) => {
                break;
            }
            None => todo!(),
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
        "`file_name` should be either the name of a file in current view, \".\" or \"..\".";

    let file_name = input.split_once(char::is_whitespace);
    if file_name.is_none() {
        println!(
            "{}",
            format!(
                "Usage: `{} <file_name>`. {}",
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
