use colored::*;
use std::net::TcpStream;

use crate::common::{CommunicationAgent, ProgramOptions, ProgramRole, QuickTransferError};
use crate::messages::MESSAGE_DIR;

pub fn handle_client(program_options: ProgramOptions) -> Result<(), QuickTransferError> {
    println!(
        "Welcome to QuickTransfer!\nConnecting to server \"{}\"...",
        program_options.server_ip_address,
    );

    let mut stream = connect_to_server(&program_options)?;
    let mut agent = CommunicationAgent::new(&mut stream, ProgramRole::Client);

    agent.send_init_message()?;
    agent.receive_message_header(MESSAGE_DIR)?;

    let dir_description_length = agent.receive_message_length()?;
    let dir_description = agent.receive_directory_description(dir_description_length)?;

    println!(
        "{}",
        format!(
            "Successfully connected to {}!",
            program_options.server_ip_address.on_green().white()
        )
        .green()
        .bold()
    );
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
            print!("{}", format!("{}\t", position.name).bright_blue());
        } else {
            print!("{}", format!("{}\t", position.name).white());
        }
    }
    println!();

    Ok(())
}

fn connect_to_server(program_options: &ProgramOptions) -> Result<TcpStream, QuickTransferError> {
    let stream = TcpStream::connect((
        program_options.server_ip_address.clone(),
        program_options.port,
    ));

    if let Err(e) = stream {
        if let Some(code) = e.raw_os_error() {
            if code == 111 {
                return Err(QuickTransferError::new_from_string(format!("Couldn't connect to server \"{}\". Make sure this is a correct address and the server is running QuickTransfer on port {}.", &program_options.server_ip_address, &program_options.port)));
            }
        }
        return Err(QuickTransferError::new(
            "An error occurred while creating a connection. Please try again.",
        ));
    }

    Ok(stream.unwrap())
}
