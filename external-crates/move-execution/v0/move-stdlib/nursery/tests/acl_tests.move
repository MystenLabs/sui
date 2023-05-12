#[test_only]
module std::aclTests {
    use std::acl;
    use std::signer;
    use std::vector;
    use std::unit_test;

    struct Data has key {
        value: u64,
        write_acl: acl::ACL
    }

    #[test]
    fun test_success() acquires Data {
        let (alice, bob) = create_two_signers();
        let acl = acl::empty();
        acl::add(&mut acl, signer::address_of(&alice));
        move_to(&alice, Data {value: 0, write_acl: acl});

        let alice_data = borrow_global_mut<Data>(signer::address_of(&alice));
        acl::add(&mut alice_data.write_acl, signer::address_of(&bob));
        acl::remove(&mut alice_data.write_acl, signer::address_of(&bob));
    }

    #[test]
    #[expected_failure(abort_code = 7, location = std::acl)]
    fun test_add_failure() acquires Data {
        let (alice, bob) = create_two_signers();
        let acl = acl::empty();
        acl::add(&mut acl, signer::address_of(&alice));
        move_to(&alice, Data {value: 0, write_acl: acl});

        let alice_data = borrow_global_mut<Data>(signer::address_of(&alice));
        acl::add(&mut alice_data.write_acl, signer::address_of(&bob));
        acl::add(&mut alice_data.write_acl, signer::address_of(&bob));
    }

    #[test]
    #[expected_failure(abort_code = 263, location = std::acl)]
    fun test_remove_failure() acquires Data {
        let (alice, bob) = create_two_signers();
        let acl = acl::empty();
        acl::add(&mut acl, signer::address_of(&alice));
        move_to(&alice, Data {value: 0, write_acl: acl});

        let alice_data = borrow_global_mut<Data>(signer::address_of(&alice));
        acl::remove(&mut alice_data.write_acl, signer::address_of(&bob));
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
