# `QuickTransfer`'s protocol
`QuickTransfer` works on TCP.
All messages' headers start with 8 bytes -- an id of the message.

1. Client send an init message to the server.
2. Server answers with a "DIR" message to the client.

## Messages structures:
1. "INIT": |8B: `INIT____`|
2. "DIR": |8B: `INIT_OK_`|8B: (length of directory contents)|?B: (directory contents)|

TODO:
1. Rozdzielenie plików
2. Czytanie n bajtów
3. Timeout? (na razie wiesza się na receive_tcp jak czeka na kolejne bajty)
4. Upakować funkcje w common w jakiegoś structa?
5. usize to musi być u64.