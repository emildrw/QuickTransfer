use argparse::{ArgumentParser, Store, StoreTrue};

mod client;
mod common;
mod server;

use common::{ProgramOptions, ProgramRole, DEFAULT_PORT};

fn parse_arguments() -> Option<ProgramOptions> {
    let mut role_server = false;
    let mut server_ip_address = String::new();
    let mut port: u16 = DEFAULT_PORT;

    let parsing_result: Result<(), i32>;

    {
        let mut argument_parser = ArgumentParser::new();
        argument_parser.set_description(
            "QuickTransfer lets you send and download files from any computer quickly.",
        );

        argument_parser.refer(&mut role_server).add_option(
            &["-s", "--server"],
            StoreTrue,
            "Run QuickTransfer in server mode",
        );
        argument_parser.refer(&mut server_ip_address).add_argument("server's address", Store, "In client mode: address, to which the program should connect (IP/domain name); in server mode: the interface on which the program should listen on (server defaults listens on all interfaces)");
        argument_parser.refer(&mut port).add_option(&["-p", "--port"], Store, "In client mode: port, to which the program should connect on the server; in server mode: port, on which the program should listen on. The value should be between 0-65535. Default value: 47842");

        parsing_result = argument_parser.parse_args();
    }

    if !role_server && server_ip_address.is_empty() {
        eprintln!("The server's address must be given in client mode.");
        return None;
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
            eprintln!(
                "A fatal error occurred while running the program: {}",
                error
            );
        }
    } else {
        //options.program_role == ProgramRole::Client

        if let Err(error) = client::handle_client(program_options) {
            eprintln!(
                "A fatal error occurred while running the program: {}",
                error
            );
        }
    }
}
