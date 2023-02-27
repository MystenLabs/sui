// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::legacy_emit_cost;
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::vm_status::StatusCode;
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    pop_arg,
    values::{Struct, StructRef, Value},
};
use smallvec::smallvec;
use std::collections::VecDeque;
use sui_types::sui_system_state::{
    create_authority_pubkey_bytes, create_multiaddr, create_narwhal_net_pubkey,
    create_narwhal_pubkey,
};

const METADATA_FIELD_NUM: usize = 17;

const E_METADATA_INVALID_PUBKEY: u64 = 1;
const E_METADATA_INVALID_NET_PUBKEY: u64 = 2;
const E_METADATA_INVALID_WORKER_PUBKEY: u64 = 3;
const E_METADATA_INVALID_NET_ADDR: u64 = 4;
const E_METADATA_INVALID_P2P_ADDR: u64 = 5;
const E_METADATA_INVALID_CONSENSUS_ADDR: u64 = 6;
const E_METADATA_INVALID_WORKER_ADDR: u64 = 7;

fn get_field_val(fields: &mut Vec<Value>) -> PartialVMResult<Value> {
    fields.pop().ok_or_else(|| {
        PartialVMError::new(StatusCode::MALFORMED).with_message(format!(
            "unexpectedly low number of fields {} in  fields in ValidatorMetadata",
            fields.len()
        ))
    })
}

fn field_to_vec_u8(fields: &mut Vec<Value>) -> PartialVMResult<Vec<u8>> {
    let val = get_field_val(fields)?;
    val.value_as::<Vec<u8>>().map_err(|_| {
        PartialVMError::new(StatusCode::MALFORMED).with_message(format!(
            "Wrong type of field {}  ValidatorMetadata",
            fields.len()
        ))
    })
}

pub fn validate_metadata(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    // unwraps safe because the interface of native function guarantees it.
    let metadata_ref = pop_arg!(args, StructRef).read_ref().unwrap();
    let mut fields: Vec<Value> = metadata_ref
        .value_as::<Struct>()
        .unwrap()
        .unpack()
        .unwrap()
        .collect();

    if fields.len() != METADATA_FIELD_NUM {
        return Err(
            PartialVMError::new(StatusCode::MALFORMED).with_message(format!(
                "unexpected number of fields {} in  fields in ValidatorMetadata",
                fields.len()
            )),
        );
    }

    // TODO: last four args are u64 values - nothing to validate?
    get_field_val(&mut fields)?;
    get_field_val(&mut fields)?;
    get_field_val(&mut fields)?;
    get_field_val(&mut fields)?;

    let worker_address = field_to_vec_u8(&mut fields)?;
    if create_multiaddr(worker_address).is_err() {
        return Ok(NativeResult::err(
            legacy_emit_cost(),
            E_METADATA_INVALID_WORKER_ADDR,
        ));
    }
    let consensus_address = field_to_vec_u8(&mut fields)?;
    if create_multiaddr(consensus_address).is_err() {
        return Ok(NativeResult::err(
            legacy_emit_cost(),
            E_METADATA_INVALID_CONSENSUS_ADDR,
        ));
    }
    let p2p_address = field_to_vec_u8(&mut fields)?;
    if create_multiaddr(p2p_address).is_err() {
        return Ok(NativeResult::err(
            legacy_emit_cost(),
            E_METADATA_INVALID_P2P_ADDR,
        ));
    }
    let net_address = field_to_vec_u8(&mut fields)?;
    if create_multiaddr(net_address).is_err() {
        return Ok(NativeResult::err(
            legacy_emit_cost(),
            E_METADATA_INVALID_NET_ADDR,
        ));
    }

    // TODO: two Urls - nothing to validate?
    get_field_val(&mut fields)?;
    get_field_val(&mut fields)?;
    // TODO: two Strings - nothing to validate?
    get_field_val(&mut fields)?;
    get_field_val(&mut fields)?;

    // TODO: nothing to validate?
    get_field_val(&mut fields)?;

    let worker_pubkey = field_to_vec_u8(&mut fields)?;
    if create_narwhal_net_pubkey(worker_pubkey.as_ref()).is_err() {
        return Ok(NativeResult::err(
            legacy_emit_cost(),
            E_METADATA_INVALID_WORKER_PUBKEY,
        ));
    }

    let network_pubkey = field_to_vec_u8(&mut fields)?;
    if create_narwhal_net_pubkey(network_pubkey.as_ref()).is_err() {
        return Ok(NativeResult::err(
            legacy_emit_cost(),
            E_METADATA_INVALID_NET_PUBKEY,
        ));
    }

    let pubkey = field_to_vec_u8(&mut fields)?;
    // apparently pubkey is used in two different contexts with two different conversion routines
    if create_narwhal_pubkey(pubkey.as_ref()).is_err()
        || create_authority_pubkey_bytes(pubkey.as_ref()).is_err()
    {
        return Ok(NativeResult::err(
            legacy_emit_cost(),
            E_METADATA_INVALID_PUBKEY,
        ));
    }

    // an address - nothing to validate
    get_field_val(&mut fields)?;

    if !fields.is_empty() {
        return Err(
            PartialVMError::new(StatusCode::MALFORMED).with_message(format!(
                "unexpectedly high number of fields {} in  fields in ValidatorMetadata",
                METADATA_FIELD_NUM + fields.len()
            )),
        );
    }

    // TODO: what should the cost of this be?
    let cost = legacy_emit_cost();
    Ok(NativeResult::ok(cost, smallvec![]))
}
