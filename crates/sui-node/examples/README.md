# Sui Node Builder Example

This example demonstrates how to use the `NodeBuilder` to create and run a Sui node.

## Prerequisites

- Rust toolchain installed
- Sui repository cloned

## Running the Example

1. First, make sure you're in the root directory of the Sui repository.

2. Create a genesis blob file (if you don't have one):
```bash
cargo run --bin sui genesis --force --write-config crates/sui-node/examples/node_config.yaml
```

3. Run the example:
```bash
cargo run --example node_builder_test -p sui-node
```

The example will:
1. Initialize a node with the configuration from `crates/sui-node/examples/node_config.yaml`
2. Start the node and keep it running
3. Wait for Ctrl+C to initiate shutdown

## Configuration

The example uses a basic configuration file at `crates/sui-node/examples/node_config.yaml`. You can modify this configuration to change:
- Database path
- Network address
- Metrics address
- JSON-RPC address
- Genesis file location
- Pruning settings

## Adding Custom Extensions

You can add your own extensions by implementing functions that take an `ExExContext` and return a `Result`. See the `example_extension` function in `node_builder_test.rs` for a simple example. 