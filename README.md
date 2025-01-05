# QuickTransfer
QuickTransfer allows you to quickly upload and download files from any computer.

## Compilation
QuickTransfer is compiled with Rust compiler. You can run the following command:
```sh
cargo build -r
```

## Platforms tested
QuickTransfer has been tested on Linux (Ubuntu) and Windows 11. However, when establishing a server on Windows 11, the interface on which the server should run, must be specified.

## Description
QuickTransfer can be run either in **client** or **server** mode. In the first case, the program tries to connect to the given server (by their IP address) and upon successfully connected, it starts communicating with the server. Server listens on a given interface and once a client is connected, it handles it and exists.

QuickTransfer supports **AES256** encryption. To encrypt a connection, a `256`-bit (i.e. `32`-byte) file is needed. Then, the client and server should be run with the `KEY` option equal to path to this file. If server is run with a key, it can handle encrypted connections (so if a client without encryption connects, the connection is handled in non-encrypted mode). But when the server is not run with the `KEY` option and client wants an encrypted connection, the server sends a message indicating that it does not support encryption. 

## Program options
Program can be run with the following command:
```
./QuickTransfer [OPTIONS] [SERVER'S ADDRESS]
```
Positional arguments:
- `server's address` -- In client mode: **address**, to which the program should connect (IP/domain name); in server mode: the **interface** (as the host's address) on which the program should listen on (server defaults listens on all interfaces). Argument required.
Optional arguments:
- `-h, --help` -- Show this help message and exit
- `-s, --server` -- Run QuickTransfer in server mode
- `-p, --port PORT` -- In client mode: port, to which the program should connect on the server; in server mode: port, on which the program should listen on. The value should be between 0-65535. Default: `47842`
- `-r, --root ROOT` -- Specify, which directory will be the root of filesystem shared with clients (as a server). Default: `./`
- `-t, --timeout TIMEOUT` -- Specify timeout (in seconds) for waiting for the whole message. Default: `5`
- `-k, --key KEY`, Path to 32-byte file holding an AES key (pure bytes). If not specified, the connection won't be encrypted.;

## Program operation
QuickTransfer provides an intuitive input/output system for operating with files on the server (from client). There are some commands that user may use for that purpose:
- `cd <directory_name>` -- Change directory to `directory_name` (can be a path, including `..`; note: you cannot go higher that the root directory in which the server is being run).
- `ls` -- Display current directory contents.
- `download <file_path>` -- Download the file from `file_path` (relative to current view) to current directory (i.e. on which QuickTransfer has been run). If the file exists, it will be overwritten.
- `upload <file_path>` -- Upload the file from `file_path` (relative to current directory, i.e. on which QuickTransfer has been run) to directory in current view (overrides files). If the file exists, it will be overwritten.
- `mkdir <directory_name>` -- Create a new directory in current location.
- `mv <file_dir_path> <new_name>` --  Rename a file/directory.
- `rm <file_dir_path>` -- Remove a file/empty directory (recursive removal is not allowed due to security reasons).
- `exit; disconnect; quit` -- Gracefully disconnect and exit QuickTransfer.

## Program protocol
`QuickTransfer` works on **TCP**.

All messages exchanged within client and server have headers: they are a sequence of 8 bytes -- an id of the message type.

### Messages sent between client and server
#### Messages structures
- Bare messages: (sent directly on TCP):
    - "INIT": `| 8B: INIT____ |` -- sent by client
    - "INIT_ENC": `| 8B: INIT_ENC |` -- sent by client
    - "NOT_ENC": `|8B: NOT__ENC |` -- sent by server
    - "OK": `|8B: OK______ |` -- sent by server
- Messages sent in encrypted message structure or unencrypted message structure:
    - "DIR": `| 8B: DIR_____ | 8B: (length of directory contents) | ?B: (directory contents) |`  -- sent by server
    - "CD": `| 8B: CD______ | 8B: (length of directory name) | ?B: (directory name) |` -- sent by client
    - "CDANSWER": `| 8B: CDANSWER | 8B: (length of the answer)| ?B: (answer) |` -- sent by server
    - "LS": `| 8B: LS______ |` -- sent by client
    - "DOWNLOAD": `| 8B: DOWNLOAD | 8B: (length of file name) | ?B: (file name) |` -- sent by client
    - "DOWNLOAD_FAIL": `| 8B: DOWN_FAIL | 8B: (length of the answer) | ?B: (answer) |` -- sent by server
    - "DOWNLOAD_SUCCESS": `| 8B: DOWN_SUCC | 8B: (length of the file) | ?B: (file content) |` -- sent by server
    - "UPLOAD": `| 8B: UPLOAD__ | 8B: (length of file name) | ?B: (file name) | 8B: (length of the file) | ?B: (file content) |` -- sent by client
    - "UPLOAD_RESULT": `| 8B: UPLOADRE | 8B: (length of the answer) | ?B: (answer) |`  -- sent by server
    - "MKDIR": `| 8B: MKDIR___ | 8B: (length of the name) | ?B: (name) |` -- sent by client
    - "MKDIRANS": `| 8B: MKDIR___ | 8B: (length of the answer) | ?B: (answer) |` -- sent by server
    - "RENAME:": `| 8B: RENAME__ | 8B: (length of the file/dir name) | ?B: (name) | 8B: (length of the new name) | ?B: (name) |` -- sent by client
    - "RENAME_ANSWER": `| 8B: RENAMEAN | 8B: (length of the answer) | ?B: (answer) |` -- sent by server
    - "REMOVE": `| 8B: REMOVE__ | 8B: (length of file/dir name) | ?B: (name) |` -- sent by client
    - "REMOVE_ANSWER": `| 8B: REMOVEAN | 8B: (length of the answer) | ?B: (answer) |` -- sent by server
    - "DISCONNECT": `| 8B: DISCONN_ |` -- sent by client

#### Message exchange process
1. Client sends an INIT message or INIT_ENC to the server depending on whether the client wants the connection encrypted or not
2. Server responds with:
    - "NOT_ENC" -- that means that server does not handle encrypted connections, server closes the connection
    - "OK" - server can handle this connection
3. Server answers with a DIR message to the client and waits for a message from client.
4. Client sends one of following messages:
    1. Client sends a LS command:
        1. Server answers with a DIR.
        1. Go to step (3).
    2. Client sends a CD command:
        1. Server responds with a CDANSWER.
        2. Go to step (3).
    3. Client sends a DOWNLOAD:
        1. Server responds DOWNLOAD_SUCCESS or DOWNLOAD_FAIL command.
        2. Go to step (3).
    4. Client sends an UPLOAD:
        1. Server sends UPLOAD_RESULT after full upload.
        2. Go to step (3).
    5. Client sends a MKDIR:
        1. Server responds with a MKDIRANS.
        2. Go to step (3).
    6. Client sends a RENAME:
        1. Server responds with a RENAME_ANSWER.
        2. Go to step (3).
    7. Client sends a REMOVE:
        1. Server responds with a REMOVE_ANSWER.
        2. Go to step (3).
    8. Client sends a DISCONNECT:
        1. Server closes the connection and exits.
        2. Client also closes the connection and exits. 

### Important notes
- **If a file to be downloaded/uploaded already exists, it'll be overridden.**
- If QuickTransfer doesn't have rights to modify/write a file, then an error occurs:
    - It that was the server: it sends a DOWNLOAD_FAIL to the client
    - It was the client: program tries to send a DISCONNECT message **and disconnects**.
