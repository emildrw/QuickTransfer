# `QuickTransfer`'s protocol
`QuickTransfer` works on TCP.
All messages' headers start with 8 bytes -- an id of the message.

1. Client send an init message to the server.
2. Server answers with a "INITOK__" message to the client.

## Messages structures:
1. "Init": |8B: `INIT____`|
2. "Init ok": |8B: `INIT_OK_`|8B: (length of directory contents)|?B: (directory contents)|

TODO:
1. Rozdzielenie plików
2. Czytanie n bajtów
3. Timeout? (na razie wiesza się na receive_tcp jak czeka na kolejne bajty)