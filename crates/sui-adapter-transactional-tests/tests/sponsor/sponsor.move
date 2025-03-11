// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --accounts A B --addresses test=0x0

//# publish
module test::sponsor;

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

// success, regular transaction, not sponsored
//# run test::sponsor::is_sponsored --sender A --args false

// abort(100), regular transaction, not sponsored
//# run test::sponsor::is_sponsored --sender A --args true

// abort(100), regular transaction, not sponsored
//# run test::sponsor::check_sponsor --sender A --args @A

// abort(100), regular transaction, not sponsored
//# run test::sponsor::check_sponsor --sender A --args @B

// success, sponsored transaction
//# programmable --sender A --sponsor B --inputs true
//> test::sponsor::is_sponsored(Input(0))

// abort(101), sponsored transaction
//# programmable --sender A --sponsor B --inputs false
//> test::sponsor::is_sponsored(Input(0))

// abort(101), wrong sponsor
//# programmable --sender A --sponsor B --inputs @A
//> test::sponsor::check_sponsor(Input(0))

// success, sponsored transaction with correct sponsor
//# programmable --sender A --sponsor B --inputs @B
//> test::sponsor::check_sponsor(Input(0))
