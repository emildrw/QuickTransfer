use core::str;
use std::net::{TcpListener, TcpStream};

use crate::common::ProgramOptions;
use crate::common::{receive_tcp, ProgramRole, QuickTransferError, STREAM_BUFFER_SIZE};

pub fn handle_server(program_options: ProgramOptions) -> Result<(), QuickTransferError> {
    eprintln!("Hello from server!, {}", program_options.server_ip_address);

    let listener = create_a_listener(&program_options)?;

    // For now, the server operates one client at a time.
    for stream in listener.incoming() {
        // The specifications says that stream will never return an error, hence the unwrap() will never panic:
        handle_client_as_a_server(&program_options, &mut stream.unwrap())?;

        // For now, operate one client and exit:
        break;
    }

    eprintln!("Hey");

    Ok(())
}

fn create_a_listener(program_options: &ProgramOptions) -> Result<TcpListener, QuickTransferError> {
    let listener = TcpListener::bind((
        program_options.server_ip_address.clone(),
        program_options.port,
    ));

    if listener.is_err() {
        return Err(QuickTransferError::new(
            "An error occurred while creating a server. Please try again.",
        ));
    }
    Ok(listener.unwrap())
}

fn handle_client_as_a_server(
    program_options: &ProgramOptions,
    stream: &mut TcpStream,
) -> Result<(), QuickTransferError> {
    let mut buffer = [0_u8; STREAM_BUFFER_SIZE];

    receive_tcp(stream, &mut buffer, ProgramRole::Server)?;

    eprintln!("Server read: {}", str::from_utf8(&buffer).unwrap());

    Ok(())
}
