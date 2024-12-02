# `QuickTransfer`'s protocol
`QuickTransfer` works on **TCP**.
All messages' headers start with 8 bytes -- an id of the message.

## Messages sent between client and server:
### Messages structures:
- "INIT": |8B: `INIT____`| -- sent by client
- "DIR": |8B: `DIR_____`|8B: (length of directory contents)|?B: (directory contents)|  -- sent by server
- "CD": |8B: `CD______`|8B: (length of directory name)|?B: (directory name)| -- sent by client
- "CDANSWER": |8B: `CDANSWER`|8B: (length of the answer)|?B: (answer) -- sent by server
- "LS": |8B: `LS______`| -- sent by client
- "DOWNLOAD": |8B: `DOWNLOAD`|8B: (length of file name)|?B: (file name)| -- sent by client
- "DOWNLOAD_FAIL": |8B: `DOWN_FAIL`|8B: (length of the answer)|?B: (answer)| -- sent by server
- "DOWNLOAD_SUCCESS": |8B: `DOWN_SUCC`|8B: (length of the file)|?B: (file content)| -- sent by server
- "UPLOAD": |8B: `UPLOAD__`|8B: (length of file name)|?B: (file name)|8B: (length of the file)|?B: (file content)| -- sent by client
- "UPLOAD_RESULT": |8B: `UPLOADRE`|8B: (length of the answer)|?B: (answer)| -- sent by server
- "DISCONNECT": |8B: `DISCONN_`|

### Initialization:
1. Client sends an init message to the server.
2. Server answers with a "DIR" message to the client.

### Messages exchange:
- When client sends "CD", server responses with "CDANSWER".
- When client sends "LS", server responses with "DIR".
- When client sends "DOWNLOAD", server responses with "DOWNLOAD_FAIL" oraz "DOWNLOAD_SUCCESS".

IMPORTANT POINTS:
- If a file to be downloaded/uploaded already exist, it'll be overridden.

TODO:
- Timeout? (na razie wiesza się na receive_tcp jak czeka na kolejne bajty)
- ważne: usize <= u64 (ale raczej usize == u64)
- asynchroniczność w kliencie i wiele w serwerze
- Zmiana nazwy pliku, usuwanie go
- Szyfrowane połączenie?
- zamykanie połączenia przez serwer