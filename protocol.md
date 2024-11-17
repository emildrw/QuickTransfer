# `QuickTransfer`'s protocol
`QuickTransfer` works on TCP.
All messages' headers start with 8 bytes -- an id of the message.

1. Client send an "INIT____" message to the server.