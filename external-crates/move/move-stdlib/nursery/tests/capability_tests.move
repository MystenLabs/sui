#[test_only]
module std::capability_tests {
    use std::capability;
    use std::signer;
    use std::unit_test;
    use std::vector;

    struct Feature has drop {}

    #[test]
    fun test_success() {
        let owner = create_signer();
        capability::create(&owner, &Feature{});
        let _cap = capability::acquire(&owner, &Feature{});
    }

    #[test]
    #[expected_failure(abort_code = 0x60000, location = std::capability)]
    fun test_failure() {
        let (owner, other) = create_two_signers();
        capability::create(&owner, &Feature{});
        let _cap = capability::acquire(&other, &Feature{});
    }

    #[test]
    fun test_delegate_success() {
        let (owner, delegate) = create_two_signers();
        capability::create(&owner, &Feature{});
        let cap = capability::acquire(&owner, &Feature{});
        capability::delegate(cap, &Feature{}, &delegate);
        let _delegate_cap = capability::acquire(&delegate, &Feature{});
    }

    #[test]
    #[expected_failure(abort_code = 0x60000, location = std::capability)]
    fun test_delegate_failure_after_revoke() {
        let (owner, delegate) = create_two_signers();
        capability::create(&owner, &Feature{});
        let cap = capability::acquire(&owner, &Feature{});
        capability::delegate(copy cap, &Feature{}, &delegate); // the copy should NOT be needed
        capability::revoke(cap, &Feature{}, signer::address_of(&delegate));
        let _delegate_cap = capability::acquire(&delegate, &Feature{});
    }

    #[test_only]
    fun create_signer(): signer {
        vector::pop_back(&mut unit_test::create_signers_for_testing(1))
    }

    #[test_only]
    fun create_two_signers(): (signer, signer) {
        let signers = &mut unit_test::create_signers_for_testing(2);
        (vector::pop_back(signers), vector::pop_back(signers))
    }
}
