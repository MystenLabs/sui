#[test_only]
module DiemFramework::DiemBlockTests {
    use DiemFramework::Genesis;
    use DiemFramework::DiemBlock;

    // TODO: the error code doesn't seem correct, juding by the name of the test.
    #[test(tc = @TreasuryCompliance, dr = @DiemRoot, account = @0x100)]
    #[expected_failure(abort_code = 1, location = DiemFramework::DiemTimestamp)]
    fun invalid_initialization_address(account: signer, tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        DiemBlock::initialize_block_metadata(&account);
    }
}
