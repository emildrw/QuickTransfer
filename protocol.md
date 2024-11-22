# `QuickTransfer`'s protocol
`QuickTransfer` works on **TCP**.
All messages' headers start with 8 bytes -- an id of the message.

1. Client sends an init message to the server.
2. Server answers with a "DIR" message to the client.

## Messages structures:
1. "INIT": |8B: `INIT____`|
2. "DIR": |8B: `DIR_____`|8B: (length of directory contents)|?B: (directory contents)|
3. "CD": |8B: `CD______`|8B: (length of directory name)|?B: (directory name)|

TODO:
- Timeout? (na razie wiesza się na receive_tcp jak czeka na kolejne bajty)
- usize <= u64
- wybrać inny folder przy uruchamianiu niż aktualny
- read_exact
- dwa wątki w kliencie (może tokio?)