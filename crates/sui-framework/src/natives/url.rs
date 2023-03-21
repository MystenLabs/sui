// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{legacy_emit_cost, legacy_length_cost};
use move_binary_format::errors::PartialVMResult;
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    pop_arg,
    values::{Struct, Value},
};
use smallvec::smallvec;
use std::collections::VecDeque;
use url::Url;

pub fn validate_url(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    let url_bytes = pop_arg!(args, Vec<u8>);
    let url_str = unsafe { std::str::from_utf8_unchecked(&url_bytes) };
    let cost = legacy_emit_cost();

    if let Err(_err) = Url::parse(url_str) {
        return Ok(NativeResult::err(cost, 0));
    }

    Ok(NativeResult::ok(cost, smallvec![]))
}

pub fn parse_url_internal(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    let url_bytes = pop_arg!(args, Vec<u8>);
    let url_str = unsafe { std::str::from_utf8_unchecked(&url_bytes) };

    let result = match Url::parse(url_str) {
        Ok(url) => {
            // the url scheme's `0x2::ascii::String` struct
            let scheme = Value::struct_(Struct::pack([Value::vector_u8(
                url.scheme().as_bytes().to_vec(),
            )]));

            // the url path's `0x2::ascii::String` struct
            let path = Value::struct_(Struct::pack([Value::vector_u8(
                url.path().as_bytes().to_vec(),
            )]));

            // The vector content of the host's `0x1::option::Option`
            // using the `Value::vector_for_testing_only` as it's currently the easiest way to do create move's vector of `Value`
            // it should the replaced when the proper API is ready.
            let inner_host = match url.host() {
                Some(host) => {
                    let host_str = Value::struct_(Struct::pack([Value::vector_u8(
                        host.to_string().into_bytes(),
                    )]));

                    Value::vector_for_testing_only([host_str])
                }
                None => Value::vector_for_testing_only([]),
            };

            let host = Value::struct_(Struct::pack([inner_host]));

            // The vector content of the port's `0x1::option::Option`
            let inner_port = match url.port() {
                Some(port) => Value::vector_u64([port as u64]),
                None => Value::vector_u64([]),
            };

            let port = Value::struct_(Struct::pack([inner_port]));

            // Vector of `0x2::vec_map::Entry` for each of the url query param
            let params = url
                .query_pairs()
                .map(|(key, val)| {
                    let key_bytes = key.to_string().into_bytes();
                    let val_bytes = val.to_string().into_bytes();

                    let key_struct = Value::struct_(Struct::pack([Value::vector_u8(key_bytes)]));
                    let val_struct = Value::struct_(Struct::pack([Value::vector_u8(val_bytes)]));

                    Value::struct_(Struct::pack([key_struct, val_struct]))
                })
                .collect::<Vec<Value>>();

            // The url params' `0x2::vec_map_VecMap` struct.
            let params = Value::struct_(Struct::pack([Value::vector_for_testing_only(params)]));

            let cost = legacy_length_cost();
            NativeResult::ok(
                cost,
                smallvec![Value::struct_(Struct::pack([
                    scheme, host, path, port, params
                ]))],
            )
        }
        _ => NativeResult::err(legacy_emit_cost(), 0),
    };

    Ok(result)
}
