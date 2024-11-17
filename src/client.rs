use std::net::TcpStream;

use crate::common::{
    receive_message_header, send_tcp, ProgramOptions, ProgramRole, QuickTransferError,
};
use crate::messages::{MESSAGE_INIT, MESSAGE_INIT_OK};

pub fn handle_client(program_options: ProgramOptions) -> Result<(), QuickTransferError> {
    eprintln!("Hello from client!");

    let mut stream = connect_to_server(&program_options)?;

    eprintln!("Client connected!");

    send_tcp(
        &mut stream,
        MESSAGE_INIT.as_bytes(),
        true,
        ProgramRole::Client,
    )?;

    eprintln!("Client sent an INIT message!");

    receive_message_header(&mut stream, MESSAGE_INIT_OK, ProgramRole::Client)?;

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
