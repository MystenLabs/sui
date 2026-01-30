// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// ZK Range proofs (bulletproofs)
module sui::bulletproofs;

use sui::group_ops::Element;
use sui::ristretto255::Point;

public fun verify_range_proof(proof: &vector<u8>, range: u8, commitment: &Element<Point>): bool {
    verify_bulletproof_ristretto255(proof, range, commitment.bytes())
}

native fun verify_bulletproof_ristretto255(
    proof: &vector<u8>,
    range: u8,
    commitment: &vector<u8>,
): bool;

#[test]
fun test_bulletproof() {
    let proof =
        x"e0048a98b0f1545cab04cfb43b995a7079a35ae4a472bb2fac4d679311ab7f104531a4107479a775f79155b3b814ad7b65c34deca847e8ef9339b97d61fb83ea4c5978e1e4dab81dc027a405699617fe938d718ebfbe165ac7cf6fb10029bd07d73e5c232000f7242fdec95e66f5b80eefa1a79d18d0d8f1c50c63b5529f41fb4311f94a701a7ce42a47772e21aa7cb01a269fa4db6195f93bbde35fd35f36270b095f3b3834ccef5d833ec94e8b66598436bf9a751970cfb3ecf7738a69bd1971049d97cdf9fcf1e9a29e5028008986b7d251196fb0f8c63316903a54b1ee4b6107a62a9af25bd4d5ba55866abb132d6056e36e3c8adb508245692cd2c5a4f797187a4de2a28d2e86cf3da9ec1fa6f224564c94e471829fbc4b60cf02953cfce36f48cbdec4e1310dd30994361b71ebc8ca2ad80eb2dab0e5da6be0088d30a584071ceb34b2eaa8b2e5a026e19114f02b7c48a826584208be3c5f50828c8753877d9496195a266a412f4104fa510b9f12e4ceb9e0786a201b4eeefcfad962b3f423ce632e46ca20033990fc8ac484cd8cde10dbdd639c16c3259060ceb46aa2f463c82cac924a37e1915725381fb2aa3cfe00652a71707eb20da99f7aa9fdb40e58ee93d11ccb71b30b8573a1498c3cf776a08945413ee19e7697b6c53191594c40649fa20880fed18145e64ab58726420737708e3136e17907d1b32f48258a5509608c49827c3bc14f2458148ad3d9b87352af617f4fd168be1547d4914069d04dedcfbce83908b249d30dcbf4da212861171ccaa94cf4262b8628f073de9d63080e0598cfb37f824f4111b68fe625b56362a951c62fd02839ca3437a002f2110c";
    let commitment = x"c026d2b1790b3391f991ad4a2ad62e3ae5db6da3eeb2280aa83bd6018fe3967b";
    assert!(verify_bulletproof_ristretto255(&proof, 32, &commitment));
}
