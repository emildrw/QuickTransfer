use colored::*;
use std::net::{TcpListener, TcpStream};

use crate::common::messages::MESSAGE_INIT;
use crate::common::{CommunicationAgent, ProgramOptions, ProgramRole, QuickTransferError};

pub fn handle_server(program_options: ProgramOptions) -> Result<(), QuickTransferError> {
    println!(
        "Welcome to QuickTransfer!\nWaiting for clients to connect on port {}...",
        program_options.port,
    );

    let listener = create_a_listener(&program_options)?;

    // For now, the server operates one client at a time.
    for stream in listener.incoming() {
        // The specifications says that stream will never return an error, hence the unwrap() will never panic:
        handle_client_as_a_server(&program_options, &mut stream.unwrap())?;

        // For now, operate one client and exit:
        break;
    }

    Ok(())
}

fn create_a_listener(program_options: &ProgramOptions) -> Result<TcpListener, QuickTransferError> {
    let listener = TcpListener::bind((
        program_options.server_ip_address.clone(),
        program_options.port,
    ));

    if listener.is_err() {
        return Err(QuickTransferError::ServerCreation);
    }
    Ok(listener.unwrap())
}

fn handle_client_as_a_server(
    _program_options: &ProgramOptions,
    stream: &mut TcpStream,
) -> Result<(), QuickTransferError> {
    // The documentation doesn't say, when this functions returns an error, so let's assume that never:
    let client_address = stream.peer_addr().unwrap();
    let mut agent = CommunicationAgent::new(stream, ProgramRole::Server);

    agent.receive_message_header(MESSAGE_INIT)?;
    agent.send_directory_description(&String::from("./"))?;

    println!(
        "{}",
        format!(
            "A new client ({}) has connected!",
            client_address
                .ip()
                .to_canonical()
                .to_string()
                .on_green()
                .white()
        )
        .green()
        .bold()
    );

    Ok(())
}
