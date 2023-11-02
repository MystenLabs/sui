# Distributed Execution

## Running the code

Deploy a local testbed with the following command (1 sequence worker and several execution workers):

```bash
cargo run --bin benchmark_executor -- testbed
```

Spawn a single shard:

```bash
cargo run --bin benchmark_executor -- run --id <id>
```

## Running tests

Please always run tests to ensure your recent modifications did not break anything:

```bash
cargo test -p sui-distributed-execution -- --nocapture
```

## Changing execution parameters

### The config file

You need to have a config file in JSON format. See for example `/crates/sui-distributed-execution/configs/1sw4ew.json`. Currently, the id of the sequence worker needs to be `0` and `1` through `4` for the execution wokers (5 workers in total). This config file specifies for each worker:

- its ID (an integer)
- its kind (`SW` for Sequence Worker or `EW` for Execution Worker)
- its IP address and port for communicating to other workers
- its metrics socket address
- a set `attr` of String-to-String key-pair attributes. Currently, this must include `"mode"`, which must be set to `"channel"`.

For legacy reasons, the `attr` field of `EW` workers currently also needs to include the following fields, even though they are not used. This constraint will be removed soon.

- `"config"`: the path to a .yaml fullnode configuration file
- `"metrics-address"`: (only if running more than one worker on a single machine) this needs to be different for each worker. This metric address is not used but must point to an unused port.
- `"execute"`: how many checkpoints to execute
