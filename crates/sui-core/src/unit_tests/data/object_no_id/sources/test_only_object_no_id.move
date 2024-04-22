// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module object_no_id::test_only_object_no_id {
    #[test_only]
    public struct NotObject has key {f: u64}

    #[test]
    fun bad_share() {
        sui::transfer::share_object(NotObject{f: 42});
    }
}
