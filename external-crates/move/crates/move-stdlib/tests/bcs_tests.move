// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module std::bcs_tests;

use std::bcs;

public struct Box<T> has copy, drop, store { x: T }
public struct Box3<T> has copy, drop, store { x: Box<Box<T>> }
public struct Box7<T> has copy, drop, store { x: Box3<Box3<T>> }
public struct Box15<T> has copy, drop, store { x: Box7<Box7<T>> }
public struct Box31<T> has copy, drop, store { x: Box15<Box15<T>> }
public struct Box63<T> has copy, drop, store { x: Box31<Box31<T>> }
public struct Box127<T> has copy, drop, store { x: Box63<Box63<T>> }

#[test]
fun bcs_address() {
    let addr = @0x0000000000000000000000000000000089b9f9d1fadc027cf9532d6f99041522;
    let expected_output = x"0000000000000000000000000000000089b9f9d1fadc027cf9532d6f99041522";
    assert!(bcs::to_bytes(&addr) == expected_output);
}

#[test]
fun bcs_bool() {
    let expected_output = x"01";
    assert!(bcs::to_bytes(&true) == expected_output);
}

#[test]
fun bcs_u8() {
    let expected_output = x"01";
    assert!(bcs::to_bytes(&1u8) == expected_output);
}

#[test]
fun bcs_u16() {
    let expected_output = x"0100";
    assert!(bcs::to_bytes(&1u16) == expected_output);
}

#[test]
fun bcs_u32() {
    let expected_output = x"01000000";
    assert!(bcs::to_bytes(&1u32) == expected_output);
}

#[test]
fun bcs_u64() {
    let expected_output = x"0100000000000000";
    assert!(bcs::to_bytes(&1) == expected_output);
}

#[test]
fun bcs_u128() {
    let expected_output = x"01000000000000000000000000000000";
    assert!(bcs::to_bytes(&1u128) == expected_output);
}

#[test]
fun bcs_u256() {
    let expected_output = x"0100000000000000000000000000000000000000000000000000000000000000";
    assert!(bcs::to_bytes(&1u256) == expected_output);
}

#[test]
fun bcs_vec_u8() {
    let v = x"0f";
    let expected_output = x"010f";
    assert!(bcs::to_bytes(&v) == expected_output);
}

fun box3<T>(x: T): Box3<T> {
    Box3 { x: Box { x: Box { x } } }
}

fun box7<T>(x: T): Box7<T> {
    Box7 { x: box3(box3(x)) }
}

fun box15<T>(x: T): Box15<T> {
    Box15 { x: box7(box7(x)) }
}

fun box31<T>(x: T): Box31<T> {
    Box31 { x: box15(box15(x)) }
}

fun box63<T>(x: T): Box63<T> {
    Box63 { x: box31(box31(x)) }
}

fun box127<T>(x: T): Box127<T> {
    Box127 { x: box63(box63(x)) }
}

#[test]
fun encode_128() {
    bcs::to_bytes(&box127(true));
}

#[test]
#[expected_failure]
// failes due to VM max value depth
fun encode_129() {
    bcs::to_bytes(&Box { x: box127(true) });
}
