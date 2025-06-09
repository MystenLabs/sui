    WARNING: this is a tool in development and it is not supported at the moment.
    We are working on it and it will be coming soon.
    If you have feedback or errors please open an issue in github.
    Usage and implementation may change without notice.

## Replay Tool
A tool to replay a transaction given a digest or a list of digests.
```
Replay executed transactions.

Usage: sui-replay-2 [OPTIONS]

Options:
  -d, --digest <DIGEST>              Transaction digest to replay
      --digests-path <DIGESTS_PATH>  File containing a list of digest, one per line
  -n, --node <NODE>                  RPC of the fullnode used to replay the transaction [default: mainnet]
  -s, --show-effects                 Show transaction effects
  -v, --verify                       Verify transaction execution matches what was executed on chain
      --trace [<TRACE>]              Provide a directory to collect tracing. Or defaults to `<cur_dir>/.replay/<digest>`
  -h, --help                         Print help
```
Digests are provided either on the command line or in a file that contains digests one per line.<br>
By default the tool only execute transactions. To verify that the executed transaction produces
the same effects as the one executed by the system one must pass the `-v` flag. That is likely to be
the default in time but it's off for convenience.<br>
`-s` shows effects and gas status to help inspect a transaction.
`-n` specifies the chain. Right now only mainnet and a custom chain are supported. In time a node would be
likely one of `mainnet`, `testnet`, `devnet` or `custom(url)`.

### Installation

You can install the replay tool by executing the following command which will result in depositing the tool's binary into the `~/.cargo/bin` directory:
```bash
cargo install --git https://github.com/MystenLabs/sui sui-replay-2
```

If you want to enable the ability to trace transaction execution during replay, build the tool with this additional flag: `--features tracing`

### Tracing and Debugging Replayed Transactions

We have added preliminary support for tracing transactions that can be replayed by the tool. You can trace a given transaction by adding the `--trace` flag when replaying this transaction, and the resulting trace additional metadata needed to trace-debug this transaction will be deposited in the subdirectory of the default `.replay` "root" directory named after the transaction digest. You can also follow `--trace` flag with an optional alternative "root" directory path.

The actual trace debugging of a given transaction is supported by the Move Trace Debugger VSCode [extension](https://marketplace.visualstudio.com/items?itemName=mysten.move-trace-debug) available in the VSCode Marketplace. This extension is also installed automatically when the "main" Move [extension](https://marketplace.visualstudio.com/items?itemName=mysten.move) is installed. Once the extension is installed, you can trace-debug a transaction by opening its trace file in VSCode and starting a "conventional" debugging session.


### Code Organization
A replay tool is an invocation to [`execute_transaction_to_effects`](http://github.com/MystenLabs/sui/blob/main/sui-execution/src/executor.rs#L26-L53) which contains info related to the transaction and info a node obtained while being live (running). For instance, a validator does not have a store for epochs, it lives/operates in an epoch. <br>
When replaying, however, we run into a past epoch and we need information about that epoch as in rpg, start timestamp and more.<br><p>
`replay_interface.rs` defines the traits the replay tool uses. Those are the functions needed
by a replay tool in order to run a transaction.
The 3 main interfaces are:
- `TransactionStore`: it's capable of returning `TransactionData`, `TransactionEffects` and `chakpoint` of the transaction identified by the transaction digest
- `EpochStore`: return information about a given epoch. Intersting sata in the epoch table is: epoch, protocol config, rgp, system packages versions for the epoch and more. The executor API (`execute_transaction_to_effects` cannot be called without that info)
- `ObjectStore`: objects and packages are loaded vi this trait
</p>
<p>

`data_store.rs` is a simple and useful implementation of the replay interfaces.
</p>
<p>

`replay_txn.rs` is where data to replay the transaction is loaded. `TransactionData` and `TransactionEffects` are handled here to retrieve the objects and packages used.
</p>
<p>

`execution.rs` is the wrapper around `execute_transaction_to_effects` and the implementation of the storage traits
is there (`BackingPackageStore`, `ObjectStore`, `ChildObjectResolver`)
</p>
<p>

`tracing.rs` contains the code to save tracing and later more information about the transaction executed.
</p>
