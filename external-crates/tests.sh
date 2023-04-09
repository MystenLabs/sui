# Run tests for external crates
echo "Running Move tests in external-crates"
cd move
echo "Excluding prover and evm Move tests"
cargo nextest run -E '!package(move-to-yul) and !package(move-prover) and !test(prove) and !test(simple_build_with_docs)'