// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::event_tests {
    use sui::event;
    use sui::test_utils::assert_eq;

    public struct S1(u64) has copy, drop;

    public struct S2(u64) has copy, drop;

    #[test]
    fun test_no_emit() {
        assert!(event::events_by_type<S1>().is_empty());
        assert_eq(event::num_events(), 0)
    }

    #[test]
    fun test_emit_homogenous() {
        let e0 = S1(0);
        event::emit(e0);
        assert_eq(event::events_by_type<S1>()[0], e0);
        assert!(event::events_by_type<S2>().is_empty());
        assert_eq(event::num_events(), 1);
        let e1 = S1(1);
        event::emit(e1);
        assert_eq(event::events_by_type<S1>()[0], e0);
        assert_eq(event::events_by_type<S1>()[1], e1);
        assert_eq(event::num_events(), 2)
    }

    #[test]
    fun test_emit_duplicate() {
        let e0 = S1(0);
        event::emit(e0);
        event::emit(e0);
        assert_eq(event::num_events(), 2);
        assert_eq(event::events_by_type<S1>().length(), 2)
    }

     #[test]
    fun test_emit_heterogenous() {
        let e0 = S1(0);
        let e1 = S2(1);
        event::emit(e0);
        event::emit(e1);
        assert_eq(event::events_by_type<S1>()[0], e0);
        assert_eq(event::events_by_type<S2>()[0], e1);
        assert_eq(event::num_events(), 2);
        let e2 = S2(2);
        let e3 = S1(3);
        event::emit(e2);
        event::emit(e3);
        assert_eq(event::events_by_type<S2>()[1], e2);
        assert_eq(event::events_by_type<S1>()[1], e3);
        assert_eq(event::num_events(), 4);
    }
}
