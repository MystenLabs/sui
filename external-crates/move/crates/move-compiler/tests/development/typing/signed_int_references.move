// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    // Immutable reference to signed int
    fun imm_ref() {
        let x: i64 = 42i64;
        let _r: &i64 = &x;
    }

    // Mutable reference to signed int
    fun mut_ref() {
        let mut x: i64 = 42i64;
        let r: &mut i64 = &mut x;
        *r = 100i64;
    }

    // Dereference
    fun deref() {
        let x: i32 = 10i32;
        let r = &x;
        let _v: i32 = *r;
    }

    // Reference in struct field
    struct RefHolder has drop {
        val: i64,
    }

    fun ref_struct_field() {
        let mut s = RefHolder { val: 1i64 };
        s.val = -1i64;
        let _v = s.val;
    }

    // Pass by reference
    fun modify(x: &mut i64) {
        *x = -(*x);
    }

    fun call_modify() {
        let mut x: i64 = 5i64;
        modify(&mut x);
    }
}
