Whenever we release a new binary that has a new protocol version to a network, we should go to the tip of that network's branch, and run this command:
```
cargo run --bin sui-framework-snapshot
```
And the commit the changes to `main` branch to record it.
It's important to ensure that each protocol version should correspond to a unique snapshot among all networks,
i.e. for the same protocol version, testnet and mainnet should contain identical framework bytecode.
