# sui-fork

This crate implements the `sui fork` CLI command, which starts a local single-process
Sui network forked from testnet or mainnet at a given checkpoint.

## Architecture

- `lib.rs` - `ForkedNode` struct and `run()` entry point
- `store.rs` - `ForkedStore` implementing `SimulatorStore` with local + remote fallback
- `bootstrap.rs` - Bootstrap logic to create a `ForkedNode` from remote state
- `rpc.rs` - JSON-RPC server via jsonrpsee
- `snapshot.rs` - State snapshot and revert support

## Testing

```bash
SUI_SKIP_SIMTESTS=1 cargo nextest run -p sui-fork
```
