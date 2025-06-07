// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::accumulator;

use std::type_name;
use sui::dynamic_field;
use sui::object::sui_accumulator_root_address;

public struct Key has copy, drop, store {
    address: address,
    ty: vector<u8>,
}

public(package) fun get_accumulator_field_name<T>(address: address): Key {
    let ty = type_name::get_with_original_ids<T>().into_string().into_bytes();
    Key { address, ty }
}

public(package) fun get_accumulator_field_address<T>(address: address): address {
    let key = get_accumulator_field_name<T>(address);
    dynamic_field::hash_type_and_key(sui_accumulator_root_address(), key)
}