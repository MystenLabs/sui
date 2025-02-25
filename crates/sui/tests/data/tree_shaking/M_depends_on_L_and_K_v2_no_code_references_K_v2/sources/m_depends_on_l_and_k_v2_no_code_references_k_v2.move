// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module m_depends_on_l_and_k_v2_no_code_references_k_v2::m_depends_on_l_and_k_v2_no_code_references_k_v2 {
    public fun m() {
        let k = 1;
        l_depends_on_k::l_depends_on_k::l();
    }
}
