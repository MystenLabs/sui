// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {

    public struct NBase has copy, drop { t: u64 }

    public struct PBase(u64) has copy, drop;

    public struct NPoly<T> has copy, drop { t: T }

    public struct PPoly<T>(T) has copy, drop;

    public fun make_nbase(): NBase {
        NBase { t: 0 }
    }

    public fun make_pbase(): PBase {
        PBase(0)
    }

    public fun make_npoly<T>(t: T): NPoly<T> {
        NPoly { t }
    }

    public fun make_ppoly<T>(t: T): PPoly<T> {
        PPoly(t)
    }

    public fun test_00(s: NBase): u64 {
        match (s) {
           NBase { mut t } => {
                t = t + 1;
                t
           },
        }
    }

    public fun test_01(s: NBase): u64 {
        match (s) {
           NBase { t: mut x } => {
                x = x + 1;
                x
           },
        }
    }

    public fun test_02(s: PBase): u64 {
        match (s) {
           PBase(mut x) => {
               x = x + 1;
               x
           }
        }
    }

    public fun test_03(s: NPoly<NBase>): u64 {
        match (s) {
           NPoly { t : NBase { mut t } } => {
                t = t + 1;
                t
           },
        }
    }

    public fun test_04(s: NPoly<NBase>): u64 {
        match (s) {
           NPoly { t : NBase { t: mut x } } => {
                x = x + 1;
                x
           },
        }
    }

    public fun test_05(s: NPoly<PBase>): u64 {
        match (s) {
           NPoly { t : PBase(mut x) } => {
                x = x + 1;
                x
           },
        }
    }

    public fun test_06(s: PPoly<NBase>): u64 {
        match (s) {
           PPoly(NBase { t: mut x }) => {
                x = x + 1;
                x
           },
        }
    }

    public fun test_07(s: PPoly<PBase>): u64 {
        match (s) {
           PPoly(PBase(mut x)) => {
                x = x + 1;
                x
           },
        }
    }

}
