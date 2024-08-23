// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::object_runtime::ObjectRuntime;
use crate::NativesCostTable;
use fastcrypto::error::{FastCryptoError, FastCryptoResult};
use fastcrypto::groups::{
    bls12381 as bls, FromTrustedByteArray, GroupElement, HashToGroupElement, MultiScalarMul,
    Pairing,
};
use fastcrypto::serde_helpers::ToFromByteArray;
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::gas_algebra::InternalGas;
use move_core_types::vm_status::StatusCode;
use move_vm_runtime::native_charge_gas_early_exit;
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    pop_arg,
    values::{Value, VectorRef},
};
use smallvec::smallvec;
use std::collections::VecDeque;

pub const NOT_SUPPORTED_ERROR: u64 = 0;
pub const INVALID_INPUT_ERROR: u64 = 1;
pub const INPUT_TOO_LONG_ERROR: u64 = 2;

fn is_supported(context: &NativeContext) -> bool {
    context
        .extensions()
        .get::<ObjectRuntime>()
        .protocol_config
        .enable_group_ops_native_functions()
}

// Gas related structs and functions.

#[derive(Clone)]
pub struct GroupOpsCostParams {
    // costs for decode and validate
    pub bls12381_decode_scalar_cost: Option<InternalGas>,
    pub bls12381_decode_g1_cost: Option<InternalGas>,
    pub bls12381_decode_g2_cost: Option<InternalGas>,
    pub bls12381_decode_gt_cost: Option<InternalGas>,
    // costs for decode, add, and encode output
    pub bls12381_scalar_add_cost: Option<InternalGas>,
    pub bls12381_g1_add_cost: Option<InternalGas>,
    pub bls12381_g2_add_cost: Option<InternalGas>,
    pub bls12381_gt_add_cost: Option<InternalGas>,
    // costs for decode, sub, and encode output
    pub bls12381_scalar_sub_cost: Option<InternalGas>,
    pub bls12381_g1_sub_cost: Option<InternalGas>,
    pub bls12381_g2_sub_cost: Option<InternalGas>,
    pub bls12381_gt_sub_cost: Option<InternalGas>,
    // costs for decode, mul, and encode output
    pub bls12381_scalar_mul_cost: Option<InternalGas>,
    pub bls12381_g1_mul_cost: Option<InternalGas>,
    pub bls12381_g2_mul_cost: Option<InternalGas>,
    pub bls12381_gt_mul_cost: Option<InternalGas>,
    // costs for decode, div, and encode output
    pub bls12381_scalar_div_cost: Option<InternalGas>,
    pub bls12381_g1_div_cost: Option<InternalGas>,
    pub bls12381_g2_div_cost: Option<InternalGas>,
    pub bls12381_gt_div_cost: Option<InternalGas>,
    // costs for hashing
    pub bls12381_g1_hash_to_base_cost: Option<InternalGas>,
    pub bls12381_g2_hash_to_base_cost: Option<InternalGas>,
    pub bls12381_g1_hash_to_cost_per_byte: Option<InternalGas>,
    pub bls12381_g2_hash_to_cost_per_byte: Option<InternalGas>,
    // costs for encoding the output + base cost for MSM (the |q| doublings) but not decoding
    pub bls12381_g1_msm_base_cost: Option<InternalGas>,
    pub bls12381_g2_msm_base_cost: Option<InternalGas>,
    // cost that is multiplied with the approximated number of additions
    pub bls12381_g1_msm_base_cost_per_input: Option<InternalGas>,
    pub bls12381_g2_msm_base_cost_per_input: Option<InternalGas>,
    // limit the length of the input vectors for MSM
    pub bls12381_msm_max_len: Option<u32>,
    // costs for decode, pairing, and encode output
    pub bls12381_pairing_cost: Option<InternalGas>,
}

macro_rules! native_charge_gas_early_exit_option {
    ($native_context:ident, $cost:expr) => {{
        use move_binary_format::errors::PartialVMError;
        use move_core_types::vm_status::StatusCode;
        native_charge_gas_early_exit!(
            $native_context,
            $cost.ok_or_else(|| {
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Gas cost for group ops is missing".to_string())
            })?
        );
    }};
}

// Next should be aligned with the related Move modules.
#[repr(u8)]
enum Groups {
    BLS12381Scalar = 0,
    BLS12381G1 = 1,
    BLS12381G2 = 2,
    BLS12381GT = 3,
}

impl Groups {
    fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Groups::BLS12381Scalar),
            1 => Some(Groups::BLS12381G1),
            2 => Some(Groups::BLS12381G2),
            3 => Some(Groups::BLS12381GT),
            _ => None,
        }
    }
}

fn parse_untrusted<G: ToFromByteArray<S> + FromTrustedByteArray<S>, const S: usize>(
    e: &[u8],
) -> FastCryptoResult<G> {
    G::from_byte_array(e.try_into().map_err(|_| FastCryptoError::InvalidInput)?)
}

fn parse_trusted<G: ToFromByteArray<S> + FromTrustedByteArray<S>, const S: usize>(
    e: &[u8],
) -> FastCryptoResult<G> {
    G::from_trusted_byte_array(e.try_into().map_err(|_| FastCryptoError::InvalidInput)?)
}

// Binary operations with 2 different types.
fn binary_op_diff<
    G1: ToFromByteArray<S1> + FromTrustedByteArray<S1>,
    G2: ToFromByteArray<S2> + FromTrustedByteArray<S2>,
    const S1: usize,
    const S2: usize,
>(
    op: impl Fn(G1, G2) -> FastCryptoResult<G2>,
    a1: &[u8],
    a2: &[u8],
) -> FastCryptoResult<Vec<u8>> {
    let e1 = parse_trusted::<G1, S1>(a1)?;
    let e2 = parse_trusted::<G2, S2>(a2)?;
    let result = op(e1, e2)?;
    Ok(result.to_byte_array().to_vec())
}

// Binary operations with the same type.
fn binary_op<G: ToFromByteArray<S> + FromTrustedByteArray<S>, const S: usize>(
    op: impl Fn(G, G) -> FastCryptoResult<G>,
    a1: &[u8],
    a2: &[u8],
) -> FastCryptoResult<Vec<u8>> {
    binary_op_diff::<G, G, S, S>(op, a1, a2)
}

// TODO: Since in many cases more than one group operation will be performed in a single
// transaction, it might be worth caching the affine representation of the group elements and use
// them to save conversions.

/***************************************************************************************************
 * native fun internal_validate
 * Implementation of the Move native function `internal_validate(type: u8, bytes: &vector<u8>): bool`
 *   gas cost: group_ops_decode_bls12381_X_cost where X is the requested type
 **************************************************************************************************/

pub fn internal_validate(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 2);

    let cost = context.gas_used();
    if !is_supported(context) {
        return Ok(NativeResult::err(cost, NOT_SUPPORTED_ERROR));
    }

    let bytes_ref = pop_arg!(args, VectorRef);
    let bytes = bytes_ref.as_bytes_ref();
    let group_type = pop_arg!(args, u8);

    let cost_params = &context
        .extensions()
        .get::<NativesCostTable>()
        .group_ops_cost_params
        .clone();

    let result = match Groups::from_u8(group_type) {
        Some(Groups::BLS12381Scalar) => {
            native_charge_gas_early_exit_option!(context, cost_params.bls12381_decode_scalar_cost);
            parse_untrusted::<bls::Scalar, { bls::Scalar::BYTE_LENGTH }>(&bytes).is_ok()
        }
        Some(Groups::BLS12381G1) => {
            native_charge_gas_early_exit_option!(context, cost_params.bls12381_decode_g1_cost);
            parse_untrusted::<bls::G1Element, { bls::G1Element::BYTE_LENGTH }>(&bytes).is_ok()
        }
        Some(Groups::BLS12381G2) => {
            native_charge_gas_early_exit_option!(context, cost_params.bls12381_decode_g2_cost);
            parse_untrusted::<bls::G2Element, { bls::G2Element::BYTE_LENGTH }>(&bytes).is_ok()
        }
        _ => false,
    };

    Ok(NativeResult::ok(cost, smallvec![Value::bool(result)]))
}

/***************************************************************************************************
 * native fun internal_add
 * Implementation of the Move native function `internal_add(type: u8, e1: &vector<u8>, e2: &vector<u8>): vector<u8>`
 *   gas cost: group_ops_bls12381_X_add_cost where X is the requested type
 **************************************************************************************************/
pub fn internal_add(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 3);

    let cost = context.gas_used();
    if !is_supported(context) {
        return Ok(NativeResult::err(cost, NOT_SUPPORTED_ERROR));
    }

    let e2_ref = pop_arg!(args, VectorRef);
    let e2 = e2_ref.as_bytes_ref();
    let e1_ref = pop_arg!(args, VectorRef);
    let e1 = e1_ref.as_bytes_ref();
    let group_type = pop_arg!(args, u8);

    let cost_params = &context
        .extensions()
        .get::<NativesCostTable>()
        .group_ops_cost_params
        .clone();

    let result = match Groups::from_u8(group_type) {
        Some(Groups::BLS12381Scalar) => {
            native_charge_gas_early_exit_option!(context, cost_params.bls12381_scalar_add_cost);
            binary_op::<bls::Scalar, { bls::Scalar::BYTE_LENGTH }>(|a, b| Ok(a + b), &e1, &e2)
        }
        Some(Groups::BLS12381G1) => {
            native_charge_gas_early_exit_option!(context, cost_params.bls12381_g1_add_cost);
            binary_op::<bls::G1Element, { bls::G1Element::BYTE_LENGTH }>(|a, b| Ok(a + b), &e1, &e2)
        }
        Some(Groups::BLS12381G2) => {
            native_charge_gas_early_exit_option!(context, cost_params.bls12381_g2_add_cost);
            binary_op::<bls::G2Element, { bls::G2Element::BYTE_LENGTH }>(|a, b| Ok(a + b), &e1, &e2)
        }
        Some(Groups::BLS12381GT) => {
            native_charge_gas_early_exit_option!(context, cost_params.bls12381_gt_add_cost);
            binary_op::<bls::GTElement, { bls::GTElement::BYTE_LENGTH }>(|a, b| Ok(a + b), &e1, &e2)
        }
        _ => Err(FastCryptoError::InvalidInput),
    };

    match result {
        Ok(bytes) => Ok(NativeResult::ok(cost, smallvec![Value::vector_u8(bytes)])),
        // Since all Element<G> are validated on construction, this error should never happen unless the requested type is wrong or inputs are invalid.
        Err(_) => Ok(NativeResult::err(cost, INVALID_INPUT_ERROR)),
    }
}

/***************************************************************************************************
 * native fun internal_sub
 * Implementation of the Move native function `internal_sub(type: u8, e1: &vector<u8>, e2: &vector<u8>): vector<u8>`
 *   gas cost: group_ops_bls12381_X_sub_cost where X is the requested type
 **************************************************************************************************/
pub fn internal_sub(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 3);

    let cost = context.gas_used();
    if !is_supported(context) {
        return Ok(NativeResult::err(cost, NOT_SUPPORTED_ERROR));
    }

    let e2_ref = pop_arg!(args, VectorRef);
    let e2 = e2_ref.as_bytes_ref();
    let e1_ref = pop_arg!(args, VectorRef);
    let e1 = e1_ref.as_bytes_ref();
    let group_type = pop_arg!(args, u8);

    let cost_params = &context
        .extensions()
        .get::<NativesCostTable>()
        .group_ops_cost_params
        .clone();

    let result = match Groups::from_u8(group_type) {
        Some(Groups::BLS12381Scalar) => {
            native_charge_gas_early_exit_option!(context, cost_params.bls12381_scalar_sub_cost);
            binary_op::<bls::Scalar, { bls::Scalar::BYTE_LENGTH }>(|a, b| Ok(a - b), &e1, &e2)
        }
        Some(Groups::BLS12381G1) => {
            native_charge_gas_early_exit_option!(context, cost_params.bls12381_g1_sub_cost);
            binary_op::<bls::G1Element, { bls::G1Element::BYTE_LENGTH }>(|a, b| Ok(a - b), &e1, &e2)
        }
        Some(Groups::BLS12381G2) => {
            native_charge_gas_early_exit_option!(context, cost_params.bls12381_g2_sub_cost);
            binary_op::<bls::G2Element, { bls::G2Element::BYTE_LENGTH }>(|a, b| Ok(a - b), &e1, &e2)
        }
        Some(Groups::BLS12381GT) => {
            native_charge_gas_early_exit_option!(context, cost_params.bls12381_gt_sub_cost);
            binary_op::<bls::GTElement, { bls::GTElement::BYTE_LENGTH }>(|a, b| Ok(a - b), &e1, &e2)
        }
        _ => Err(FastCryptoError::InvalidInput),
    };

    match result {
        Ok(bytes) => Ok(NativeResult::ok(cost, smallvec![Value::vector_u8(bytes)])),
        // Since all Element<G> are validated on construction, this error should never happen unless the requested type is wrong or inputs are invalid.
        Err(_) => Ok(NativeResult::err(cost, INVALID_INPUT_ERROR)),
    }
}

/***************************************************************************************************
 * native fun internal_mul
 * Implementation of the Move native function `internal_mul(type: u8, e1: &vector<u8>, e2: &vector<u8>): vector<u8>`
 *   gas cost: group_ops_bls12381_X_mul_cost where X is the requested type
 **************************************************************************************************/
pub fn internal_mul(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 3);

    let cost = context.gas_used();
    if !is_supported(context) {
        return Ok(NativeResult::err(cost, NOT_SUPPORTED_ERROR));
    }

    let e2_ref = pop_arg!(args, VectorRef);
    let e2 = e2_ref.as_bytes_ref();
    let e1_ref = pop_arg!(args, VectorRef);
    let e1 = e1_ref.as_bytes_ref();
    let group_type = pop_arg!(args, u8);

    let cost_params = &context
        .extensions()
        .get::<NativesCostTable>()
        .group_ops_cost_params
        .clone();

    let result = match Groups::from_u8(group_type) {
        Some(Groups::BLS12381Scalar) => {
            native_charge_gas_early_exit_option!(context, cost_params.bls12381_scalar_mul_cost);
            binary_op::<bls::Scalar, { bls::Scalar::BYTE_LENGTH }>(|a, b| Ok(b * a), &e1, &e2)
        }
        Some(Groups::BLS12381G1) => {
            native_charge_gas_early_exit_option!(context, cost_params.bls12381_g1_mul_cost);
            binary_op_diff::<
                bls::Scalar,
                bls::G1Element,
                { bls::Scalar::BYTE_LENGTH },
                { bls::G1Element::BYTE_LENGTH },
            >(|a, b| Ok(b * a), &e1, &e2)
        }
        Some(Groups::BLS12381G2) => {
            native_charge_gas_early_exit_option!(context, cost_params.bls12381_g2_mul_cost);
            binary_op_diff::<
                bls::Scalar,
                bls::G2Element,
                { bls::Scalar::BYTE_LENGTH },
                { bls::G2Element::BYTE_LENGTH },
            >(|a, b| Ok(b * a), &e1, &e2)
        }
        Some(Groups::BLS12381GT) => {
            native_charge_gas_early_exit_option!(context, cost_params.bls12381_gt_mul_cost);
            binary_op_diff::<
                bls::Scalar,
                bls::GTElement,
                { bls::Scalar::BYTE_LENGTH },
                { bls::GTElement::BYTE_LENGTH },
            >(|a, b| Ok(b * a), &e1, &e2)
        }
        _ => Err(FastCryptoError::InvalidInput),
    };

    match result {
        Ok(bytes) => Ok(NativeResult::ok(cost, smallvec![Value::vector_u8(bytes)])),
        // Since all Element<G> are validated on construction, this error should never happen unless the requested type is wrong or inputs are invalid.
        Err(_) => Ok(NativeResult::err(cost, INVALID_INPUT_ERROR)),
    }
}

/***************************************************************************************************
 * native fun internal_div
 * Implementation of the Move native function `internal_div(type: u8, e1: &vector<u8>, e2: &vector<u8>): vector<u8>`
 *   gas cost: group_ops_bls12381_X_div_cost where X is the requested type
 **************************************************************************************************/
pub fn internal_div(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 3);

    let cost = context.gas_used();
    if !is_supported(context) {
        return Ok(NativeResult::err(cost, NOT_SUPPORTED_ERROR));
    }

    let e2_ref = pop_arg!(args, VectorRef);
    let e2 = e2_ref.as_bytes_ref();
    let e1_ref = pop_arg!(args, VectorRef);
    let e1 = e1_ref.as_bytes_ref();
    let group_type = pop_arg!(args, u8);

    let cost_params = &context
        .extensions()
        .get::<NativesCostTable>()
        .group_ops_cost_params
        .clone();

    let result = match Groups::from_u8(group_type) {
        Some(Groups::BLS12381Scalar) => {
            native_charge_gas_early_exit_option!(context, cost_params.bls12381_scalar_div_cost);
            binary_op::<bls::Scalar, { bls::Scalar::BYTE_LENGTH }>(|a, b| b / a, &e1, &e2)
        }
        Some(Groups::BLS12381G1) => {
            native_charge_gas_early_exit_option!(context, cost_params.bls12381_g1_div_cost);
            binary_op_diff::<
                bls::Scalar,
                bls::G1Element,
                { bls::Scalar::BYTE_LENGTH },
                { bls::G1Element::BYTE_LENGTH },
            >(|a, b| b / a, &e1, &e2)
        }
        Some(Groups::BLS12381G2) => {
            native_charge_gas_early_exit_option!(context, cost_params.bls12381_g2_div_cost);
            binary_op_diff::<
                bls::Scalar,
                bls::G2Element,
                { bls::Scalar::BYTE_LENGTH },
                { bls::G2Element::BYTE_LENGTH },
            >(|a, b| b / a, &e1, &e2)
        }
        Some(Groups::BLS12381GT) => {
            native_charge_gas_early_exit_option!(context, cost_params.bls12381_gt_div_cost);
            binary_op_diff::<
                bls::Scalar,
                bls::GTElement,
                { bls::Scalar::BYTE_LENGTH },
                { bls::GTElement::BYTE_LENGTH },
            >(|a, b| b / a, &e1, &e2)
        }
        _ => Err(FastCryptoError::InvalidInput),
    };

    match result {
        Ok(bytes) => Ok(NativeResult::ok(cost, smallvec![Value::vector_u8(bytes)])),
        // Since all Element<G> are validated on construction, this error should never happen unless the requested type is wrong, inputs are invalid, or a=0.
        Err(_) => Ok(NativeResult::err(cost, INVALID_INPUT_ERROR)),
    }
}

/***************************************************************************************************
 * native fun internal_hash_to
 * Implementation of the Move native function `internal_hash_to(type: u8, m: &vector<u8>): vector<u8>`
 *   gas cost: group_ops_bls12381_X_hash_to_base_cost + group_ops_bls12381_X_hash_to_cost_per_byte * |input|
 *             where X is the requested type
 **************************************************************************************************/
pub fn internal_hash_to(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 2);

    let cost = context.gas_used();
    if !is_supported(context) {
        return Ok(NativeResult::err(cost, NOT_SUPPORTED_ERROR));
    }

    let m_ref = pop_arg!(args, VectorRef);
    let m = m_ref.as_bytes_ref();
    let group_type = pop_arg!(args, u8);

    if m.is_empty() {
        return Ok(NativeResult::err(cost, INVALID_INPUT_ERROR));
    }

    let cost_params = &context
        .extensions()
        .get::<NativesCostTable>()
        .group_ops_cost_params
        .clone();

    let result = match Groups::from_u8(group_type) {
        Some(Groups::BLS12381G1) => {
            native_charge_gas_early_exit_option!(
                context,
                cost_params
                    .bls12381_g1_hash_to_base_cost
                    .and_then(|base_cost| cost_params
                        .bls12381_g1_hash_to_cost_per_byte
                        .map(|per_byte| base_cost + per_byte * (m.len() as u64).into()))
            );
            Ok(bls::G1Element::hash_to_group_element(&m)
                .to_byte_array()
                .to_vec())
        }
        Some(Groups::BLS12381G2) => {
            native_charge_gas_early_exit_option!(
                context,
                cost_params
                    .bls12381_g2_hash_to_base_cost
                    .and_then(|base_cost| cost_params
                        .bls12381_g2_hash_to_cost_per_byte
                        .map(|per_byte| base_cost + per_byte * (m.len() as u64).into()))
            );
            Ok(bls::G2Element::hash_to_group_element(&m)
                .to_byte_array()
                .to_vec())
        }
        _ => Err(FastCryptoError::InvalidInput),
    };

    match result {
        Ok(bytes) => Ok(NativeResult::ok(cost, smallvec![Value::vector_u8(bytes)])),
        // Since all Element<G> are validated on construction, this error should never happen unless the requested type is wrong or inputs are invalid.
        Err(_) => Ok(NativeResult::err(cost, INVALID_INPUT_ERROR)),
    }
}

// Based on calculation from https://github.com/supranational/blst/blob/master/src/multi_scalar.c#L270
fn msm_num_of_additions(n: u64) -> u64 {
    debug_assert!(n > 0);
    let wbits = (64 - n.leading_zeros() - 1) as u64;
    let window_size = match wbits {
        0 => 1,
        1..=4 => 2,
        5..=12 => wbits - 2,
        _ => wbits - 3,
    };
    let num_of_windows = 255 / window_size + if 255 % window_size == 0 { 0 } else { 1 };
    num_of_windows * (n + (1 << window_size) + 1)
}

#[test]
fn test_msm_factor() {
    assert_eq!(msm_num_of_additions(1), 1020);
    assert_eq!(msm_num_of_additions(2), 896);
    assert_eq!(msm_num_of_additions(3), 1024);
    assert_eq!(msm_num_of_additions(4), 1152);
    assert_eq!(msm_num_of_additions(32), 3485);
}

fn multi_scalar_mul<G, const SCALAR_SIZE: usize, const POINT_SIZE: usize>(
    context: &mut NativeContext,
    scalar_decode_cost: Option<InternalGas>,
    point_decode_cost: Option<InternalGas>,
    base_cost: Option<InternalGas>,
    base_cost_per_addition: Option<InternalGas>,
    max_len: u32,
    scalars: &[u8],
    points: &[u8],
) -> PartialVMResult<NativeResult>
where
    G: GroupElement
        + ToFromByteArray<POINT_SIZE>
        + FromTrustedByteArray<POINT_SIZE>
        + MultiScalarMul,
    G::ScalarType: ToFromByteArray<SCALAR_SIZE> + FromTrustedByteArray<SCALAR_SIZE>,
{
    if points.is_empty()
        || scalars.is_empty()
        || scalars.len() % SCALAR_SIZE != 0
        || points.len() % POINT_SIZE != 0
        || points.len() / POINT_SIZE != scalars.len() / SCALAR_SIZE
    {
        return Ok(NativeResult::err(context.gas_used(), INVALID_INPUT_ERROR));
    }

    if points.len() / POINT_SIZE > max_len as usize {
        return Ok(NativeResult::err(context.gas_used(), INPUT_TOO_LONG_ERROR));
    }

    native_charge_gas_early_exit_option!(
        context,
        scalar_decode_cost.map(|cost| cost * ((scalars.len() / SCALAR_SIZE) as u64).into())
    );
    let scalars = scalars
        .chunks(SCALAR_SIZE)
        .map(parse_trusted::<G::ScalarType, { SCALAR_SIZE }>)
        .collect::<Result<Vec<_>, _>>();

    native_charge_gas_early_exit_option!(
        context,
        point_decode_cost.map(|cost| cost * ((points.len() / POINT_SIZE) as u64).into())
    );
    let points = points
        .chunks(POINT_SIZE)
        .map(parse_trusted::<G, { POINT_SIZE }>)
        .collect::<Result<Vec<_>, _>>();

    if let (Ok(scalars), Ok(points)) = (scalars, points) {
        // Checked above that len()>0
        let num_of_additions = msm_num_of_additions(scalars.len() as u64);
        native_charge_gas_early_exit_option!(
            context,
            base_cost.and_then(|base| base_cost_per_addition
                .map(|per_addition| base + per_addition * num_of_additions.into()))
        );

        let r = G::multi_scalar_mul(&scalars, &points)
            .expect("Already checked the lengths of the vectors");
        Ok(NativeResult::ok(
            context.gas_used(),
            smallvec![Value::vector_u8(r.to_byte_array().to_vec())],
        ))
    } else {
        Ok(NativeResult::err(context.gas_used(), INVALID_INPUT_ERROR))
    }
}

/***************************************************************************************************
 * native fun internal_multi_scalar_mul
 * Implementation of the Move native function `internal_multi_scalar_mul(type: u8, scalars: &vector<u8>, elements: &vector<u8>): vector<u8>`
 *   gas cost: (bls12381_decode_scalar_cost + bls12381_decode_X_cost) * N + bls12381_X_msm_base_cost +
 *             bls12381_X_msm_base_cost_per_input * num_of_additions(N)
 **************************************************************************************************/
pub fn internal_multi_scalar_mul(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 3);

    let cost = context.gas_used();
    if !is_supported(context) {
        return Ok(NativeResult::err(cost, NOT_SUPPORTED_ERROR));
    }

    let elements_ref = pop_arg!(args, VectorRef);
    let elements = elements_ref.as_bytes_ref();
    let scalars_ref = pop_arg!(args, VectorRef);
    let scalars = scalars_ref.as_bytes_ref();
    let group_type = pop_arg!(args, u8);

    let cost_params = &context
        .extensions()
        .get::<NativesCostTable>()
        .group_ops_cost_params
        .clone();

    let max_len = cost_params.bls12381_msm_max_len.ok_or_else(|| {
        PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
            .with_message("Max len for MSM is not set".to_string())
    })?;

    // TODO: can potentially improve performance when some of the points are the generator.
    match Groups::from_u8(group_type) {
        Some(Groups::BLS12381G1) => multi_scalar_mul::<
            bls::G1Element,
            { bls::Scalar::BYTE_LENGTH },
            { bls::G1Element::BYTE_LENGTH },
        >(
            context,
            cost_params.bls12381_decode_scalar_cost,
            cost_params.bls12381_decode_g1_cost,
            cost_params.bls12381_g1_msm_base_cost,
            cost_params.bls12381_g1_msm_base_cost_per_input,
            max_len,
            scalars.as_ref(),
            elements.as_ref(),
        ),
        Some(Groups::BLS12381G2) => multi_scalar_mul::<
            bls::G2Element,
            { bls::Scalar::BYTE_LENGTH },
            { bls::G2Element::BYTE_LENGTH },
        >(
            context,
            cost_params.bls12381_decode_scalar_cost,
            cost_params.bls12381_decode_g2_cost,
            cost_params.bls12381_g2_msm_base_cost,
            cost_params.bls12381_g2_msm_base_cost_per_input,
            max_len,
            scalars.as_ref(),
            elements.as_ref(),
        ),
        _ => Ok(NativeResult::err(cost, INVALID_INPUT_ERROR)),
    }
}

/***************************************************************************************************
 * native fun internal_pairing
 * Implementation of the Move native function `internal_pairing(type:u8, e1: &vector<u8>, e2: &vector<u8>): vector<u8>`
 *   gas cost: group_ops_bls12381_pairing_cost
 **************************************************************************************************/
pub fn internal_pairing(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 3);

    let cost = context.gas_used();
    if !is_supported(context) {
        return Ok(NativeResult::err(cost, NOT_SUPPORTED_ERROR));
    }

    let e2_ref = pop_arg!(args, VectorRef);
    let e2 = e2_ref.as_bytes_ref();
    let e1_ref = pop_arg!(args, VectorRef);
    let e1 = e1_ref.as_bytes_ref();
    let group_type = pop_arg!(args, u8);

    let cost_params = &context
        .extensions()
        .get::<NativesCostTable>()
        .group_ops_cost_params
        .clone();

    let result = match Groups::from_u8(group_type) {
        Some(Groups::BLS12381G1) => {
            native_charge_gas_early_exit_option!(context, cost_params.bls12381_pairing_cost);
            parse_trusted::<bls::G1Element, { bls::G1Element::BYTE_LENGTH }>(&e1).and_then(|e1| {
                parse_trusted::<bls::G2Element, { bls::G2Element::BYTE_LENGTH }>(&e2).map(|e2| {
                    let e3 = e1.pairing(&e2);
                    e3.to_byte_array().to_vec()
                })
            })
        }
        _ => Err(FastCryptoError::InvalidInput),
    };

    match result {
        Ok(bytes) => Ok(NativeResult::ok(cost, smallvec![Value::vector_u8(bytes)])),
        // Since all Element<G> are validated on construction, this error should never happen unless the requested type is wrong.
        Err(_) => Ok(NativeResult::err(cost, INVALID_INPUT_ERROR)),
    }
}
