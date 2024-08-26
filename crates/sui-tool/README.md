# sui-tool

`sui-tool` contains assorted debugging utilities for Sui.

You can build and run `sui-tool` from source with:
```sh
cargo run --bin sui-tool -- <args>
```

## `anemo` tools

You can use the anemo CLI tools to ping or call an RPC on an Anemo server. Note that (for now) this uses randomly generated keys, so a server or method that restricts access to allowlisted peers will reject connections from this tool.

Anemo networks are identified by a "server name" that the client must match. Server names you may want to use:
- Sui discovery and state sync: `sui`

### ping

Example command to ping an anemo server:

```sh
SERVER_NAME="sui"; \
ADDRESS="1.2.3.4:5678"; \
cargo run --bin sui-tool -- anemo ping --server-name "$SERVER_NAME" "$ADDRESS"
```

### call

`sui-tool` has been preconfigured to support RPC calls using [RON (Rusty Object Notation)](https://crates.io/crates/ron) for the following servivces:
- Narwhal: `PrimaryToPrimary` and `WorkerToWorker`
- Sui: `Discovery` and `StateSync`

Example command to send an RPC:

```sh
SERVER_NAME="sui"; \
ADDRESS="1.2.3.4:5678"; \
SERVICE_NAME="StateSync"; \
METHOD_NAME="GetCheckpointSummary"; \
REQUEST="BySequenceNumber(123)"; \
cargo run --bin sui-tool -- \
    anemo call --server-name "$SERVER_NAME" "$ADDRESS" "$SERVICE_NAME" "$METHOD_NAME" "$REQUEST"
```
