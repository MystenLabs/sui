// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::deny_list_tests {
    use sui::deny_list;
    use sui::test_scenario;
    use std::type_name;

    public struct X()

    #[test, expected_failure(abort_code = sui::deny_list::EInvalidAddress)]
    fun add_zero() {
        let mut ctx = tx_context::dummy();
        let mut dl = deny_list::new_for_testing(&mut ctx);
        let ty = type_name::into_string(type_name::get_with_original_ids<X>()).into_bytes();
        dl.v1_add(1, ty, deny_list::reserved_addresses()[0]); // should error
        abort 0 // should not be reached
    }

    #[test, expected_failure(abort_code = sui::deny_list::EInvalidAddress)]
    fun remove_zero() {
        let mut ctx = tx_context::dummy();
        let mut dl = deny_list::new_for_testing(&mut ctx);
        let ty = type_name::into_string(type_name::get_with_original_ids<X>()).into_bytes();
        dl.v1_add(1, ty, deny_list::reserved_addresses()[1]); // should error
        abort 0 // should not be reached
    }

    #[test]
    fun contains_zero () {
        let mut scenario = test_scenario::begin(@0);
        deny_list::create_for_test(scenario.ctx());
        scenario.next_tx(@0);
        let dl: deny_list::DenyList = scenario.take_shared();
        let ty = type_name::into_string(type_name::get_with_original_ids<X>()).into_bytes();
        let reserved = deny_list::reserved_addresses();
        let mut i = 0;
        let n = reserved.length();
        while (i < n) {
            assert!(!dl.v1_contains(1, ty, reserved[i]));
            i = i + 1;
        };
        test_scenario::return_shared(dl);
        scenario.end();
    }

}
