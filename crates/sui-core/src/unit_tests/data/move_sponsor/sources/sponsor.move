// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Test `TxContext` sponsor API
module sponsor::sponsor {
    // Check if the transaction is sponsored or not
    public fun is_sponsored(sponsored: bool, ctx: &TxContext) {
        let sponsor = ctx.sponsor();
        if (sponsored) {
            assert!(sponsor.is_some(), 100);
        } else {
            assert!(sponsor.is_none(), 101);
        }
    }

    // Check the sponsor on a sponsored transaction
    public fun check_sponsor(sponsor: address, ctx: &TxContext) {
        let txn_sponsor = ctx.sponsor();
        assert!(txn_sponsor.is_some(), 100);
        assert!(txn_sponsor.destroy_some() == sponsor, 101);
    }
}
