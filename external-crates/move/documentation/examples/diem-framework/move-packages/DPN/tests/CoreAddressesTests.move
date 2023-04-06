#[test_only]
module DiemFramework::CoreAddressesTests {
    use DiemFramework::CoreAddresses;

    #[test(dr = @DiemRoot)]
    fun test_assert_diem_root(dr: signer) {
        CoreAddresses::assert_diem_root(&dr);
    }

    #[test(tc = @TreasuryCompliance)]
    fun test_assert_treasury_compliance(tc: signer) {
        CoreAddresses::assert_treasury_compliance(&tc);
    }

    #[test(vm = @VMReserved)]
    fun test_assert_vm(vm: signer) {
        CoreAddresses::assert_vm(&vm);
    }

    #[test(ci = @CurrencyInfo)]
    fun test_assert_currency_info(ci: signer) {
        CoreAddresses::assert_currency_info(&ci);
    }
}
