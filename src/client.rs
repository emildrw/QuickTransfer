use std::error::Error;
use std::net::TcpStream;
use std::io::{Read, Write};

use crate::common::ProgramOptions;

pub fn handle_client(program_options: ProgramOptions) -> Result<(), Box<dyn Error>> {
	eprintln!("Hello from client!");

	let mut stream = TcpStream::connect((
		program_options.server_ip_address,
		program_options.port,
	))?;

	eprintln!("Client connected!");
	let init_message = String::from("INIT");
	eprintln!("Client sent an INIT message!");

	stream.write(init_message.as_bytes())?;

	Ok(())
}
