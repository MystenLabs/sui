# sui-network

## Changing an RPC service

The general process for changing an RPC service is as follows:
1. Change the service definition in the `tests/bootstrap.rs` file.
2. Run `cargo test --test bootstrap` to re-run the code generation.
   Generated rust files are in the `src/generated` directory.
3. Update any other corresponding logic that would have been affected by 
   the interface change, e.g. the server implementation of the service or
   usages of the generated client.

## Large checkpoint state sync

State sync first requests full checkpoint contents. If that download fails, it
can fetch the contents digests and then verified transactions and effects in
pages from peers supporting the pagination RPCs. Full responses larger than
128 MiB are refused; transaction pages use a 16 MiB byte budget, except that a
single transaction larger than the budget is returned alone.

A checkpoint exceeding the configured transaction-concurrency budget is admitted
when no other contents download is in flight. Its contents are marked available
only after all transactions and effects have been verified and stored.
