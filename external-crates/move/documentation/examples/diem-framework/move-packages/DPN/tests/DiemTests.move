#[test_only]
module DiemFramework::DiemTests {
    use DiemFramework::XUS::XUS;
    use DiemFramework::Genesis;
    use DiemFramework::Diem;
    use std::signer;

    struct R<T> has key { x: T }
    struct FakeCoin has store { value: u64 }

    fun fetch<T: store>(account: &signer): T acquires R {
        let R { x } = move_from<R<T>>(signer::address_of(account));
        x
    }

    fun store<T: store>(account: &signer, x: T) {
        move_to(account, R { x });
    }

    fun transmute<T1: store, T2: store>(account: &signer, x: T1): T2 acquires R {
        // There was once a bug that R<U> and R<T> shared the same storage key for U != T,
        // making it possible to perform a transmuatation.
        store(account, x);
        fetch(account)
    }

    fun become_rich(account: &signer) acquires R {
        let fake = FakeCoin { value: 400000 };
        let real = transmute(account, fake);
        Diem::destroy_zero<XUS>(real);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot, account = @0x100)]
    #[expected_failure]
    fun cannot_print_counterfeit_money(tc: signer, dr: signer, account: signer) acquires R {
        Genesis::setup(&dr, &tc);

        become_rich(&account);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 1, location = DiemFramework::DiemTimestamp)]
    fun cannot_initialize_after_genesis(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);

        Diem::initialize(&dr);
    }
}
