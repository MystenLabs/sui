// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Tests for the `personal_kiosk` module.
module kiosk::personal_kiosk_tests {
    use sui::transfer::public_share_object as share;
    use sui::kiosk_test_utils::{Self as test};
    use sui::tx_context::sender;
    use sui::kiosk;

    use kiosk::personal_kiosk;

    #[test]
    fun new_and_transfer() {
        let ctx = &mut test::ctx();
        let (kiosk, kiosk_cap) = test::get_kiosk(ctx);
        let p_cap = personal_kiosk::new(&mut kiosk, kiosk_cap, ctx);
        personal_kiosk::transfer_to_sender(p_cap, ctx);
        share(kiosk)
    }

    #[test]
    fun new_borrow() {
        let ctx = &mut test::ctx();
        let (kiosk, kiosk_cap) = test::get_kiosk(ctx);
        let p_cap = personal_kiosk::new(&mut kiosk, kiosk_cap, ctx);

        assert!(kiosk::has_access(
            &mut kiosk,
            personal_kiosk::borrow(&p_cap)
        ), 0);

        assert!(kiosk::has_access(
            &mut kiosk,
            personal_kiosk::borrow_mut(&mut p_cap)
        ), 0);

        let (kiosk_cap, borrow) = personal_kiosk::borrow_val(&mut p_cap);

        assert!(kiosk::has_access(&mut kiosk, &kiosk_cap), 0);
        assert!(kiosk::has_access(&mut kiosk, &mut kiosk_cap), 0);

        personal_kiosk::return_val(&mut p_cap, kiosk_cap, borrow);
        personal_kiosk::transfer_to_sender(p_cap, ctx);

        assert!(personal_kiosk::owner(&kiosk) == sender(ctx), 1);
        share(kiosk)
    }

    #[test, expected_failure(abort_code = kiosk::personal_kiosk::EWrongKiosk)]
    fun try_not_owned_kiosk_fail() {
        let ctx = &mut test::ctx();
        let (kiosk_1, cap_1) = test::get_kiosk(ctx);
        let (kiosk_2, cap_2) = test::get_kiosk(ctx);

        let _p1 = personal_kiosk::new(&mut kiosk_2, cap_1, ctx);
        let _p2 = personal_kiosk::new(&mut kiosk_1, cap_2, ctx);

        abort 1337
    }

    #[test, expected_failure(abort_code = kiosk::personal_kiosk::EIncorrectCapObject)]
    fun borrow_replace_cap_fail() {
        let ctx = &mut test::ctx();
        let (kiosk_1, cap_1) = test::get_kiosk(ctx);
        let p_cap_1 = personal_kiosk::new(&mut kiosk_1, cap_1, ctx);

        let (kiosk_2, cap_2) = test::get_kiosk(ctx);
        let p_cap_2 = personal_kiosk::new(&mut kiosk_2, cap_2, ctx);

        let (_kiosk_cap_1, borrow_1) = personal_kiosk::borrow_val(&mut p_cap_1);
        let (kiosk_cap_2, _borrow_2) = personal_kiosk::borrow_val(&mut p_cap_2);

        personal_kiosk::return_val(&mut p_cap_1, kiosk_cap_2, borrow_1);

        abort 1337
    }

    #[test, expected_failure(abort_code = kiosk::personal_kiosk::EIncorrectOwnedObject)]
    fun borrow_replace_target_fail() {
        let ctx = &mut test::ctx();
        let (kiosk_1, cap_1) = test::get_kiosk(ctx);
        let p_cap_1 = personal_kiosk::new(&mut kiosk_1, cap_1, ctx);

        let (kiosk_2, cap_2) = test::get_kiosk(ctx);
        let p_cap_2 = personal_kiosk::new(&mut kiosk_2, cap_2, ctx);

        let (kiosk_cap_1, borrow_1) = personal_kiosk::borrow_val(&mut p_cap_1);
        let (_kiosk_cap_2, _borrow_2) = personal_kiosk::borrow_val(&mut p_cap_2);

        personal_kiosk::return_val(&mut p_cap_2, kiosk_cap_1, borrow_1);

        abort 1337
    }

    #[test, expected_failure(abort_code = kiosk::personal_kiosk::EKioskNotOwned)]
    fun owner_fail() {
        let (kiosk, _cap) = test::get_kiosk(&mut test::ctx());
        let _owner = personal_kiosk::owner(&kiosk);
        abort 1337
    }
}
