use core::str;
use std::error::Error;
use std::net::TcpListener;
use std::io::{Read, Write};

use crate::common::STREAM_BUFFER_SIZE;
use crate::{common::ProgramOptions};

pub fn handle_server(program_options: ProgramOptions) -> Result<(), Box<dyn Error>> {
	eprintln!("Hello from server!, {}", program_options.server_ip_address);

	let listener = TcpListener::bind((
		program_options.server_ip_address,
		program_options.port,
	))?;

	// For now, the server operates one client at a time.
	for stream in listener.incoming() {
		let mut stream = stream?;

		let mut buffer = [0; STREAM_BUFFER_SIZE];
		let bytes_read = stream.read(&mut buffer)?;
		eprintln!("Server read: {}, {bytes_read}", str::from_utf8(&buffer)?);

		// For now, operate one client and exit:
		break;
	}

	eprintln!("");

	Ok(())
}
