# Run tests for external crates
echo "Running Move tests in external-crates"
cd move
echo "Excluding prover Move tests"
cargo nextest run -E '!package(move-prover) and !test(prove) and !test(run_all::simple_build_with_docs/args.txt) and !test(run_test::nested_deps_bad_parent/Move.toml)' --workspace --no-fail-fast
