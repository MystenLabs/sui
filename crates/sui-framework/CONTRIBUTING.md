This file contains useful information and troubleshooting advice for those wishing to contribute to `sui-framework` crate.

## Framework Move  source code changes

If changes need to be made to the framework's Move code, additional actions need to be taken to ensure that the system builds and runs correctly. In particular, one needs to make sure that the framework snapshot tests are up-to-date and that any new native functions are correctly handled by the [Move Prover](https://github.com/move-language/move/tree/main/language/move-prover).

### Snapshot tests update

Run the following commands in Sui's [root directory](../../) and accept the changes, if any (if you do not have `cargo-insta` command installed, please run the `cargo install cargo-insta` command first):

``` bash
cargo insta test -p sui-cost --review
cargo insta test -p sui-config --review
```

Please use your best judgment to decide if the changes between old and new versions of the snapshots look "reasonable" (e.g., a minor change in gas costs). When in doubt, please reach out to a member of Sui core team.
