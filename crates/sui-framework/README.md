# Sui Programmability with Move

This is a proof-of-concept Move standard library for Sui (`sources/`), along with several examples of programs that Sui users might want to write (`examples`). `custom_object_template.move` is a good starting point for understanding the proposed model.

To set up and build the [Sui CLI client](https://docs.sui.io/build/cli-client) needed for Move development, follow the instructions to [install Sui](https://docs.sui.io/build/install).

## To add a new native Move function

1. Add a new `./sui-framework/{name}.move` fileor find an appropriate `.move`.
2. Add the signature of the function you are adding in `{name}.move`. 
3. Add the rust implementation of the function under `./sui-framework/src/natives` under name `{name}.rs`.
4. Link the move interface with the native function in in [all_natives] (https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/src/natives/mod.rs#L23)
5. Write some tests in `{name}_tests.move` and passes `run_framework_move_unit_tests`.
6. May need to update the mock move vm value [here] (https://github.com/MystenLabs/sui/blob/276356e168047cdfce71814cb14403f4653a3656/crates/sui-core/src/unit_tests/gas_tests.rs) since the sui-framework package will increase the gas metering.
7. May need to run `cargo insta test` and `cargo insta review` since the sui-framework build will change the empty genesis config.

Note: The gas metering for native functions is currently WIP, use a dummy value for now and please open an issue with `move` label.