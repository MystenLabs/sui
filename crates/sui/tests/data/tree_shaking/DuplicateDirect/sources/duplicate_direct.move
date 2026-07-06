// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module duplicate_direct::duplicate_direct {
    public fun use_both() {
        a0::a::a();
        a1::alt::alt();
    }
}
