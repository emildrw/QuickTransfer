use colored::*;
use std::fs;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;

use crate::common::messages::{CdAnswer, MESSAGE_CD, MESSAGE_INIT};
use crate::common::{
    directory_description, CommunicationAgent, ProgramOptions, ProgramRole, QuickTransferError,
};

pub fn handle_server(program_options: ProgramOptions) -> Result<(), QuickTransferError> {
    println!(
        "Welcome to QuickTransfer!\nWaiting for clients to connect on port {}...",
        program_options.port,
    );

    let listener = create_a_listener(&program_options)?;

    // For now, the server operates one client at a time.
    // for stream in listener.incoming() {
    //     // The specifications says that stream will never return an error, hence the unwrap() will never panic:
    //     handle_client_as_a_server(stream.unwrap())?;

    //     // For now, operate one client and exit:
    //     break;
    // }
    let stream = listener.incoming().next().unwrap();
    handle_client_as_a_server(stream.unwrap())?;

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

fn handle_client_as_a_server(mut stream: TcpStream) -> Result<(), QuickTransferError> {
    // The documentation doesn't say, when this functions returns an error, so let's assume that never:
    let client_address = stream.peer_addr().unwrap();
    let mut agent = CommunicationAgent::new(&mut stream, ProgramRole::Server);

    agent.receive_message_header_check(MESSAGE_INIT)?;

    let mut current_path = PathBuf::new();
    current_path.push(".");
    let root_directory = fs::canonicalize(current_path.as_path()).unwrap();
    println!("Root: {}", root_directory.as_path().to_str().unwrap());
    agent.send_directory_description(current_path.as_path())?;

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

    loop {
        let header_received = agent.receive_message_header()?;
        match header_received.as_str() {
            MESSAGE_CD => {
                let dir_name = agent.receive_cd_message()?;

                let mut next_path = current_path.to_path_buf();
                next_path.push(dir_name);

                if !fs::exists(next_path.as_path()).unwrap() {
                    agent.send_cd_answer(&CdAnswer::DirectoryDoesNotExist)?;
                    continue;
                }

                let current = next_path.canonicalize().unwrap();
                if !current.starts_with(root_directory.clone()) {
                    agent.send_cd_answer(&CdAnswer::IllegalDirectory)?;
                    continue;
                }

                current_path = next_path;
                let directory_contents = directory_description(current_path.as_path())?;
                agent.send_cd_answer(&CdAnswer::Success(directory_contents))?;
            }
            _ => {}
        }
    }

    // Ok(())
}
