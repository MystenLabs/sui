/// Module providing well-known addresses and related logic.
///
/// > Note: this module currently defines zero-argument functions like `Self::DIEM_ROOT_ADDRESS()` using capitalization
/// > in the name, following the convention for constants. Eventually, those functions are planned to become actual
/// > global constants, once the Move language supports this feature.
module DiemFramework::CoreAddresses {
    use std::errors;
    use std::signer;

    /// The operation can only be performed by the account at 0xA550C18 (Diem Root)
    const EDIEM_ROOT: u64 = 0;
    /// The operation can only be performed by the account at 0xB1E55ED (Treasury & Compliance)
    const ETREASURY_COMPLIANCE: u64 = 1;
    /// The operation can only be performed by the VM
    const EVM: u64 = 2;
    /// The operation can only be performed by the account where currencies are registered
    const ECURRENCY_INFO: u64 = 3;

    /// Assert that the account is the Diem root address.
    public fun assert_diem_root(account: &signer) {
        assert!(signer::address_of(account) == @DiemRoot, errors::requires_address(EDIEM_ROOT))
    }
    spec assert_diem_root {
        pragma opaque;
        include AbortsIfNotDiemRoot;
    }

    /// Specifies that a function aborts if the account does not have the Diem root address.
    spec schema AbortsIfNotDiemRoot {
        account: signer;
        aborts_if signer::address_of(account) != @DiemRoot with errors::REQUIRES_ADDRESS;
    }

    /// Assert that the signer has the treasury compliance address.
    public fun assert_treasury_compliance(account: &signer) {
        assert!(
            signer::address_of(account) == @TreasuryCompliance,
            errors::requires_address(ETREASURY_COMPLIANCE)
        )
    }
    spec assert_treasury_compliance {
        pragma opaque;
        include AbortsIfNotTreasuryCompliance;
    }

    /// Specifies that a function aborts if the account does not have the treasury compliance address.
    spec schema AbortsIfNotTreasuryCompliance {
        account: signer;
        aborts_if signer::address_of(account) != @TreasuryCompliance
            with errors::REQUIRES_ADDRESS;
    }

    /// Assert that the signer has the VM reserved address.
    public fun assert_vm(account: &signer) {
        assert!(signer::address_of(account) == @VMReserved, errors::requires_address(EVM))
    }
    spec assert_vm {
        pragma opaque;
        include AbortsIfNotVM;
    }

    /// Specifies that a function aborts if the account does not have the VM reserved address.
    spec schema AbortsIfNotVM {
        account: signer;
        aborts_if signer::address_of(account) != @VMReserved with errors::REQUIRES_ADDRESS;
    }

    /// Assert that the signer has the currency info address.
    public fun assert_currency_info(account: &signer) {
        assert!(signer::address_of(account) == @CurrencyInfo, errors::requires_address(ECURRENCY_INFO))
    }
    spec assert_currency_info {
        pragma opaque;
        include AbortsIfNotCurrencyInfo;
    }

    /// Specifies that a function aborts if the account has not the currency info address.
    spec schema AbortsIfNotCurrencyInfo {
        account: signer;
        aborts_if signer::address_of(account) != @CurrencyInfo with errors::REQUIRES_ADDRESS;
    }

}
