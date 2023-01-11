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

### Native functions integration with the Move Prover

Each native function must be represented in the Move Prover model in order for the Move Prover to be able to reason about their behavior. Ideally, you would provide the actual model of a new native function expressed in the Boogie language in [crates/sui/src/sui_move/sui-natives.bpl](../sui/src/sui_move/sui-natives.bpl). Alternatively, if you do not need to have the Move Prover reason about a particular native function (you do not plan to write Move Prover specifications concerning this function), you can provide an "empty" model by defining an "stub" specification for the native function itself. A specification clause (`spec`) has the same name as the native function and the "stub" specification contains a single `pragma opaque;` statement. The specification(s) should be placed in the spec file accompanying the module file (see example below for the [bls12381](./sources/crypto/bls12381.move) module with the specifications placed in the [bls12381.spec.move](./sources/crypto/bls12381.spec.move) file):

``` rust
spec sui::bls12381 {
    // specification for the bls12381_min_sig_verify native function
    spec bls12381_min_sig_verify {
        // TODO: temporary mockup.
        pragma opaque;
    }

    // specification for the bls12381_min_pk_verify native function
    spec bls12381_min_pk_verify {
        // TODO: temporary mockup.
        pragma opaque;
    }
}
```

You can read more about defining Move Prover specifications in the documentation for the [Move Specification Language](https://github.com/move-language/move/blob/main/language/move-prover/doc/user/spec-lang.md).
