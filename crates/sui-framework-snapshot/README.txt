Whenever we release a new binary that has a new protocol version to a network, we should go to the tip of that network's branch, and run this command:
```
cargo run --bin sui-framework-snapshot -- testnet `git rev-parse HEAD`
```
And the commit the changes to `main` branch to record it.
