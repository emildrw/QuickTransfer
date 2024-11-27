use colored::*;
use std::io::{self, Write};
use std::net::TcpStream;

use crate::common::messages::{CdAnswer, MessageDirectoryContents, MESSAGE_CDANSWER, MESSAGE_DIR};
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

        match command {
            Some("cd") => {
                let invalid_error_message: &str = "`directory_name` should be either the name of a directory in current view, \".\" or \"..\".";

                let directory_name = input.split_once(char::is_whitespace);
                if directory_name.is_none() {
                    println!(
                        "{}",
                        format!("Usage: `cd <directory_name>`. {}", invalid_error_message).red()
                    );
                    continue;
                }

                let directory_name = String::from(directory_name.unwrap().1);

                if directory_name.is_empty() {
                    println!(
                        "{}",
                        format!(
                            "Note: `directory_name` cannot be empty. {}",
                            invalid_error_message
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
                            format!("Error: Directory `{}` does not exist!`", directory_name).red()
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
