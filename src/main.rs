use std::path::Path;
use argparse::{ArgumentParser, Store, StoreTrue};

mod client;
mod common;
mod server;

use common::{ProgramOptions, ProgramRole, DEFAULT_PORT};

fn parse_arguments() -> Option<ProgramOptions> {
    let mut role_server = false;
    let mut server_ip_address = String::new();
    let mut port: u16 = DEFAULT_PORT;
    let mut root_directory = String::new();

    let parsing_result: Result<(), i32>;

    {
        let mut argument_parser = ArgumentParser::new();
        argument_parser.set_description(
            "QuickTransfer lets you upload and download files from any computer quickly.",
        );

        argument_parser.refer(&mut role_server).add_option(
            &["-s", "--server"],
            StoreTrue,
            "Run QuickTransfer in server mode",
        );
        argument_parser.refer(&mut server_ip_address).add_argument("server's address", Store, "In client mode: address, to which the program should connect (IP/domain name); in server mode: the interface on which the program should listen on (server defaults listens on all interfaces). Argument required.");
        argument_parser.refer(&mut port).add_option(&["-p", "--port"], Store, "In client mode: port, to which the program should connect on the server; in server mode: port, on which the program should listen on. The value should be between 0-65535. Default: `47842`");
        argument_parser.refer(&mut root_directory).add_option(&["-r", "--root"], Store, "Specify, which directory will be the root of filesystem shared with clients (as a server). Default: `./`");

        parsing_result = argument_parser.parse_args();
    }

    if !role_server && server_ip_address.is_empty() {
        eprintln!("The server's address must be given in client mode.");
        return None;
    }

    if !root_directory.is_empty() {
        if !Path::new(&root_directory).exists() {
            eprintln!("The root directory should be a valid directory.");
            return None;
        }
    } else {
        root_directory = String::from("./");
    }

    if server_ip_address.is_empty() {
        server_ip_address = String::from("::");
    }

    if parsing_result.is_ok() {
        Some(ProgramOptions {
            program_role: if role_server {
                ProgramRole::Server
            } else {
                ProgramRole::Client
            },
            server_ip_address,
            port,
            root_directory,
        })
    } else {
        None
    }
}

fn main() {
    let program_options = parse_arguments();
    if program_options.is_none() {
        return;
    }

    let program_options = program_options.unwrap();
    if let ProgramRole::Server = program_options.program_role {
        if let Err(error) = server::handle_server(program_options) {
            eprintln!("{}", error);
        }
    } else {
        //options.program_role == ProgramRole::Client

        if let Err(error) = client::handle_client(&program_options) {
            eprintln!("{}", error);
        }
    }
}
