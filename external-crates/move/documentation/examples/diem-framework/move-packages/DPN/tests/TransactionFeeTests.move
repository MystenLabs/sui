#[test_only]
module DiemFramework::TransactionFeeTests {
    use DiemFramework::TransactionFee;
    use DiemFramework::Genesis;

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 1, location = DiemFramework::DiemTimestamp)]
    fun cannot_initialize_after_genesis(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        TransactionFee::initialize(&tc);
    }

    #[test(account = @0x100)]
    #[expected_failure(abort_code = 258, location = DiemFramework::CoreAddresses)]
    fun cannot_initialize_as_non_tc(account: signer) {
        TransactionFee::initialize(&account);
    }
}
