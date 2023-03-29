#[test_only]
module DiemFramework::XUSTests {
    use DiemFramework::XUS;
    use DiemFramework::Genesis;

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot, account = @0x100)]
    #[expected_failure(abort_code = 1, location = DiemFramework::DiemTimestamp)]
    fun cannot_recreate_market_cap(tc: signer, dr: signer, account: signer) {
        Genesis::setup(&dr, &tc);
        XUS::initialize(&account, &account);
    }
}
