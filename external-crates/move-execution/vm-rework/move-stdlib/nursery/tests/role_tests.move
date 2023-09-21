#[test_only]
module std::roleTests {
    use std::role;
    use std::vector;
    use std::unit_test;

    struct Developer has drop {}
    struct User has drop {}
    struct Admin has drop {}

    #[test]
    fun test_success() {
        let (alice, bob) = create_two_signers();
        role::assign_role<Developer>(&alice, &Developer{});
        role::assign_role<User>(&alice, &User{});
        role::assign_role<Admin>(&bob, &Admin{});

        role::revoke_role<Developer>(&alice, &Developer{});
        role::revoke_role<User>(&alice, &User{});
        role::revoke_role<Admin>(&bob, &Admin{});
    }

    #[test]
    #[expected_failure(abort_code = 0x80000, location = std::role)]
    fun test_assign_failure() {
        let alice = create_signer();
        role::assign_role<Developer>(&alice, &Developer{});
        role::assign_role<Developer>(&alice, &Developer{});
    }

    #[test]
    #[expected_failure(abort_code = 0x60000, location = std::role)]
    fun test_revoke_failure() {
        let alice = create_signer();
        role::revoke_role<Developer>(&alice, &Developer{});
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
