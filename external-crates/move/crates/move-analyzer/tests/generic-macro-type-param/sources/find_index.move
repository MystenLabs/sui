// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module GenericMacroTypeParam::a_victim {
    public fun do_ref_test(): u64 {
        let v = vector[0u64, 10, 100, 1_000];
        let mut sum = 0;
        v.do_ref!(|e| sum = sum + *e);
        sum
    }
}

module GenericMacroTypeParam::z_generic {
    public fun id_ref<K>(v: &vector<K>): &vector<K> {
        v
    }

    // Generic invocations of the same macro. The receiver is intentionally an
    // inline generic call, so the macro payload's by-value args contain K.
    // This test is a bit fragile as we cannot fully control evaluation order
    // of macro expansions in the compiler. Nevertheless, as it is, this test
    // manages to consistently expose the bug (assertion failure) prior to
    // the fix being implemented.
    public fun count<K: copy + drop>(v: &vector<K>): u64 {
        let mut n = 0;
        id_ref<K>(v).do_ref!(|_| n = n + 1);
        id_ref<K>(v).do_ref!(|_| n = n + 1);
        id_ref<K>(v).do_ref!(|_| n = n + 1);
        id_ref<K>(v).do_ref!(|_| n = n + 1);
        id_ref<K>(v).do_ref!(|_| n = n + 1);
        id_ref<K>(v).do_ref!(|_| n = n + 1);
        n
    }
}
