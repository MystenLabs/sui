# Run tests for external crates
set -e
echo "Running Move tests in external-crates"
cd move
echo "Excluding prover Move tests"
cargo nextest run -E '!test(run_all::simple_build_with_docs/args.txt) and !test(run_test::nested_deps_bad_parent/Move.toml)' --workspace --no-fail-fast
echo "Running tracing-specific tests"
cargo nextest run -p move-cli --features tracing
