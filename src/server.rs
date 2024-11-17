use core::{error, str};
use std::net::{TcpListener, TcpStream};
use std::fs::{self, DirEntry};

use byteorder::{BE, WriteBytesExt};

use crate::common::{receive_message_header, send_tcp, ProgramOptions};
use crate::common::{receive_tcp, ProgramRole, QuickTransferError};
use crate::messages::{DirectoryPosition, MessageDirectoryContents, HEADER_NAME_LENGTH, MESSAGE_INIT, MESSAGE_INIT_OK};

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
    receive_message_header(stream, MESSAGE_INIT, ProgramRole::Server)?;

    const READING_DIR_ERROR: &str = "An error occurred while reading current directory contents. Make sure the program has permission to do so. It is needed for QuickTransfer to work.";

    let paths = fs::read_dir("./");
    if let Err(_) = paths {
        return Err(QuickTransferError::new(READING_DIR_ERROR));
    }
    let paths = paths.unwrap();

    let directory_contents: Vec<Result<DirEntry, std::io::Error>> = paths.collect();
    if directory_contents.iter().any(|dir| dir.is_err()) {
        return Err(QuickTransferError::new(READING_DIR_ERROR));
    }

    let mut error_loading_contents = false;

    let directory_contents = MessageDirectoryContents(directory_contents
        .into_iter()
        .map(|dir| dir.unwrap().path())
        .map(|path: std::path::PathBuf| DirectoryPosition {
            name: String::from(path.to_str().unwrap_or_else(|| {
                error_loading_contents = true;
                "?"
            })),
            is_directory: path.is_dir(),
        })
        .collect());

    let mut init_ok_message = MESSAGE_INIT_OK.as_bytes().to_vec();

    let dir_description = bincode::serialize(&directory_contents).unwrap_or_else(|_| {
        error_loading_contents = true;
        vec![]
    });

    init_ok_message.write_u64::<BE>(dir_description.len().try_into().unwrap()).unwrap();
    init_ok_message.extend(dir_description);

    if error_loading_contents {
        return Err(QuickTransferError::new(READING_DIR_ERROR));
    }

    println!("{:?}", init_ok_message);

    send_tcp(stream, init_ok_message.as_slice(), true, ProgramRole::Server)?;

    // let deserialized: MessageDirectoryContents = bincode::deserialize(&serialized[..]).unwrap();
    // println!("{:?}", deserialized);

    Ok(())
}
