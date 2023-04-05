#[test_only]
module DiemFramework::DiemVersionTests {
    use DiemFramework::DiemVersion;
    use DiemFramework::Genesis;

    #[test(account = @0x1)]
    #[expected_failure(abort_code = 2, location = DiemFramework::CoreAddresses)]
    fun init_before_genesis(account: signer) {
        DiemVersion::initialize(&account, 0);
    }

    #[test(account = @0x1)]
    #[expected_failure(abort_code = 257, location = DiemFramework::DiemTimestamp)]
    fun set_before_genesis(account: signer) {
        DiemVersion::set(&account, 0);
    }

    #[test(account = @0x2, tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 1, location = DiemFramework::DiemTimestamp)]
    fun invalid_address_init(account: signer, tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        DiemVersion::initialize(&account, 0);
    }

    #[test(account = @0x2, tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 2, location = DiemFramework::CoreAddresses)]
    fun invalid_setting_address(account: signer, tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        DiemVersion::set(&account, 0);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 7, location = DiemVersion)]
    fun non_increasing_version(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        DiemVersion::set(&dr, 0);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    fun increasing_version(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        DiemVersion::set(&dr, 1);
    }
}
