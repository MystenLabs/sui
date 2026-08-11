// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// `verify_bulletproofs_ristretto255` is only enabled on `unknown` (devnet/local)
// chains, so on this chain the native verifies the proof and the call succeeds.
// Pinned to protocol version 126.

//# init --addresses test=0x0 --protocol-version 126 --chain unknown

//# publish
#[allow(deprecated_usage)]
module test::bulletproofs;
use sui::rangeproofs::verify_bulletproofs_ristretto255;
use sui::ristretto255;

public entry fun verify() {
    let proof =
        x"6aec1feb7c55ddd896d36e33adaaa407f3894462557b04485d7c9e3e51f0df405858ff59863c616b86786d9012145b97d086d221924eab6b5dbf59b813e9f75dec9ec334e6fe9eae3fc090de948061f21e8db40df786fe3956d6e80de12e314220597efd41aea0a4a3a2141d790df3f92d8849ee82266f7212e250d9e6d6794ee8214b4bd49bd4df66a1cd3f63608a42e5384dfb717611a74d3f9152e10e4903819043cefa206789120c108c3fd5fafb3edde2a3f56ee9e97d8d86771c9ed00937cb74f9fa98e242555a503729481aa1fb1fd699a0bbeea0ddbaa447a473160844c2ba74efac5ac0dc4363317a031db48aef28bfccf51a25beec1f49827f15350c4866cfd4e85b5315c6313b6039531931d962d0a2fbc2ead3aad5b0c4c74e2bc2159e8965d75d501bdcca5df4ed8216f040b49afe995fd2fee3d4a968e6fc7fe2a7339e352554e7f603401eb4fa125cef3a1d6ec89452924a7cf313c9356b1f8a126cf9f1fc4024ae5a7991fcb42eeb435ceaeb8af80ceb24877c3e6c30927884391927294350a5b5121f53394e3012bb07e5980c93ea204bdf6eeef64c2763c477440566bbc4ebcad271057da6bb048bd5b6a853dab280e0ede0fce80f4934e2e4933dbd12c58b9a310f70766c559ebcd394d99c5f489bc913a52d3cf99b1512c5c7e19671e00048822256f5d900f42afe3f688f1f77020fab905ee9c5d72cfc24a46e0112cf93531957258bbbbf55f7b22adf767e069add9994775021813a68180e4316f0cf494faef39c778c4e9688a9e1642f366c1af9df5c9e5811a0067c954c7699124daa7cec5ce7faf35d8ba3a110f848d66a17555db86da486d206";
    let commitment = ristretto255::g_from_bytes(
        &x"9ab1abbccce6f74dd9a062ad863e46319a8fb74df55495308ee646547fbde15e",
    );
    assert!(verify_bulletproofs_ristretto255(&proof, 32, &vector[commitment], 0));
}

//# run test::bulletproofs::verify
