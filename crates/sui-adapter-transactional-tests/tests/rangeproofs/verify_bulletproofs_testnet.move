// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// `verify_bulletproofs_ristretto255` is not enabled on `testnet`. We use an empty
// commitments vector so that `g_from_bytes` (whose ristretto255 group ops are also
// gated off on this network) is never reached; execution lands directly in the
// bulletproofs native, whose `is_supported` check aborts first with `ENotSupported`
// (code 0) in `sui::rangeproofs` -- isolating the bulletproofs feature gate.

//# init --addresses test=0x0 --chain testnet

//# publish
#[allow(deprecated_usage)]
module test::bulletproofs;
use sui::rangeproofs::verify_bulletproofs_ristretto255;
use sui::ristretto255;

public entry fun verify() {
    let commitments = vector<sui::group_ops::Element<ristretto255::G>>[];
    verify_bulletproofs_ristretto255(&vector[], 32, &commitments, 0);
}

//# run test::bulletproofs::verify
