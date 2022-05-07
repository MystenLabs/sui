# sui-network

## Changing an RPC service

The general process for changing an RPC service is as follows:
1. Change the service definition in the `tests/bootstrap.rs` file.
2. Run `cargo test --test bootstrap` to re-run the code generation.
   Generated rust files are in the `src/generated` directory.
3. Update any other corresponding logic that would have been affected by 
   the interface change, e.g. the server implementation of the service or
   usages of the generated client.
