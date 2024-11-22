use colored::*;
use std::io::{self, Write};
use std::net::TcpStream;

use crate::common::messages::MESSAGE_DIR;
use crate::common::{CommunicationAgent, ProgramOptions, ProgramRole, QuickTransferError};

pub fn handle_client(program_options: ProgramOptions) -> Result<(), QuickTransferError> {
    println!(
        "Welcome to QuickTransfer!\nFor help, type `help`.\nConnecting to server \"{}\"...",
        program_options.server_ip_address,
    );

    let mut stream = connect_to_server(&program_options)?;
    let mut agent = CommunicationAgent::new(&mut stream, ProgramRole::Client);

    agent.send_init_message()?;
    agent.receive_message_header(MESSAGE_DIR)?;

    println!(
        "{}",
        format!(
            "Successfully connected to {}!",
            program_options.server_ip_address.on_green().white()
        )
        .green()
        .bold()
    );

    receive_and_read_directory_contents(&mut agent)?;
    
    loop {
        print!("QT > ");
        io::stdout().flush().map_err(|_| QuickTransferError::StdoutError)?;
    
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
                    println!("{}", format!("Usage: `cd <directory_name>`. {}", invalid_error_message).red());
                    continue;
                }
    
                let directory_name = String::from(directory_name.unwrap().1);
                
                if directory_name.is_empty() {
                    println!("{}", format!("Note: `directory_name` cannot be empty. {}", invalid_error_message).red());
                    continue;
                } 
                if directory_name.contains('/') {
                    println!("{}", format!("Note: `directory_name` cannot be a path. {}", invalid_error_message).red());
                    continue;
                }

                agent.send_change_directory(&directory_name)?;
            },
            Some("help") => {
    
            }
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

fn receive_and_read_directory_contents(agent: &mut CommunicationAgent) -> Result<(), QuickTransferError> {
    let dir_description_length = agent.receive_message_length()?;
    let dir_description = agent.receive_directory_description(dir_description_length)?;

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
    println!();

    Ok(())
}