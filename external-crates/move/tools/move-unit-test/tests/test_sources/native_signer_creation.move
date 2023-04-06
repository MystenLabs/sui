module 0x1::M {
    use std::unit_test;
    use std::vector;
    use std::signer;

    struct A has key {}

    fun has_a(a: address): bool {
        exists<A>(a)
    }

    #[test_only]
    fun setup_storage(sig: &signer) {
        move_to(sig, A { })
    }

    #[test]
    fun test_exists() {
        let num_signers = 10;
        let i = 0;

        let signers = unit_test::create_signers_for_testing(num_signers);
        while (i < num_signers) {
            setup_storage(vector::borrow(&signers, i));
            i = i + 1;
        };

        i = 0;
        while (i < num_signers) {
            assert!(has_a(signer::address_of(vector::borrow(&signers, i))), 0);
            i = i + 1;
        }
    }

    #[test]
    fun test_doesnt_exist() {
        let num_signers = 10;
        let i = 0;

        let signers = unit_test::create_signers_for_testing(num_signers);
        while (i < num_signers) {
            setup_storage(vector::borrow(&signers, i));
            i = i + 1;
        };

        // abort to trigger a dump of storage state to make sure this is getting populated correctly
        abort 0

    }

    #[test]
    fun test_determinisim() {
        let num_signers = 10;
        let i = 0;
        let signers = unit_test::create_signers_for_testing(num_signers);
        let other_signers = unit_test::create_signers_for_testing(num_signers);

        while (i < num_signers) {
            assert!(
                signer::address_of(vector::borrow(&signers, i)) ==
                  signer::address_of(vector::borrow(&other_signers, i)),
                i
            );
            i = i + 1;
        };
    }
}
