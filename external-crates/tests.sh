# Run tests for external crates
echo "Running Move tests in external-crates"
cd move
echo "Excluding prover and evm Move tests"
cargo nextest run -E '!package(move-to-yul) and !package(move-prover) and !package(evm-exec-utils) and !test(prove) and !test(run_test::nested_deps_bad_parent/Move.toml) and !test(run_test::external/Move.toml) and !test(run_test::external_dev_dep/Move.toml) and !test(run_test::reference_safety/call_function_with_many_acquires)' --workspace --no-fail-fast
