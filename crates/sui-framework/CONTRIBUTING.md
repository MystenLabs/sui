This file contains useful information and troubleshooting advice for those wishing to contribute to `sui-framework` crate.

## Framework Move source code changes

If changes need to be made to the framework's Move code, additional actions need to be taken to ensure that the system builds and runs correctly. In particular, one needs to make sure that the framework snapshot tests are up-to-date and that any new native functions are correctly handled.

### Snapshot tests update

Run the following script from the Sui's [root directory](../../) and accept any changes (if you do not have `cargo-insta` installed, run the `cargo install cargo-insta` command first):

```bash
./scripts/update_all_snapshots.sh
```

Please use your best judgment to decide if the changes between old and new versions of the snapshots look "reasonable" (e.g., a minor change in gas costs). When in doubt, please reach out to a member of Sui core team.
