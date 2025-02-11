// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module e_depends_on_a_v1_and_on_b_depends_on_a_and_code_references_a::e_depends_on_a_v1_and_on_b_depends_on_a_and_code_references_a {
    public fun e1() {
        a::a::a1();
    }
}
