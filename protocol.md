# `QuickTransfer`'s protocol
`QuickTransfer` works on **TCP**.
All messages' headers start with 8 bytes -- an id of the message.

1. Client sends an init message to the server.
2. Server answers with a "DIR" message to the client.

## Messages structures:
1. "INIT": |8B: `INIT____`| -- sent by client
2. "DIR": |8B: `DIR_____`|8B: (length of directory contents)|?B: (directory contents)|  -- sent by server
3. "CD": |8B: `CD______`|8B: (length of directory name)|?B: (directory name)| -- sent by client
4. "CDANSWER": |8B: `CDANSWER`|8B: (length of the answer)|?B: (answer) -- sent by server
5. "LS": |8B: `LS______`| -- send by client

TODO:
- Timeout? (na razie wiesza się na receive_tcp jak czeka na kolejne bajty)
- usize <= u64
- wybrać inny folder przy uruchamianiu niż aktualny
- dwa wątki w kliencie (może tokio?)
- powrzucać te rzeczy do odbierania długości w funkcje, aby było przejrzyściej