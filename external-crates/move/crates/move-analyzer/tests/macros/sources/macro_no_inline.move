// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Macros::macro_no_inline {

    public struct MyStruct has drop {
        val: u64,
    }

    fun helper(x: u64, y: u64): u64 {
        x + y
    }

    // Macro with value params, let bindings, and function calls
    macro fun with_let($x: u64): u64 {
        let result = $x + 1;
        helper(result, $x)
    }

    // Macro with struct param - tests dot access and field resolution
    macro fun uses_struct($s: MyStruct): u64 {
        let s = $s;
        s.val + 0
    }

    // Macro with incomplete dot access - tests completion inside a non-inlined macro body
    macro fun completes_struct($s: MyStruct) {
        let s = $s;
        s.;
    }

    // Macro with lambda param - tests VarCall return type inference
    macro fun apply($x: u64, $f: |u64| -> u64): u64 {
        let result = $f($x);
        result
    }

    // Generic macro - tests abstract type params and method resolution
    public macro fun for_each<$T>($v: &vector<$T>, $body: |&$T|) {
        let v = $v;
        let mut i = 0;
        let n = v.length();
        while (i < n) {
            $body(v.borrow(i));
            i = i + 1
        }
    }
}
