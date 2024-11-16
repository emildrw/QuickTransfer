use argparse::{ArgumentParser, Store, StoreTrue};

#[derive(Debug)]
struct ProgramOptions {
    server: bool,
    server_ip_address: String,
}

fn parse_arguments() -> Option<ProgramOptions> {
    let mut server = false;
    let mut server_ip_address = String::new();

    let parsing_result: Result<(), i32>;

    {
        let mut argument_parser = ArgumentParser::new();
        argument_parser.set_description("QuickTransfer lets you send and download files from any computer quickly.");
        argument_parser.refer(&mut server_ip_address).add_argument("server's address", Store, "Address, to which the program should connect (IP/domain name).");
        argument_parser.refer(&mut server).add_option(&["-s", "--server"], StoreTrue, "Run QuickTransfer in server mode");

        parsing_result = argument_parser.parse_args();
    }

    if let Ok(_) = parsing_result {
        return Some(ProgramOptions {
            server,
            server_ip_address,
        });
    } else {
        return None;
    }
}

fn main() {
    let options = parse_arguments();

    if let None = options {
        return;
    }
    
    let options = options.unwrap();

    println!("{:?}", options);
}
