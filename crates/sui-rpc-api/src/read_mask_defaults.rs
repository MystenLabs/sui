// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::field::MessageFields;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;

use crate::ErrorReason;
use crate::RpcError;

/// Default read mask for a transaction (`get_transaction`, `list_transactions`).
pub const TRANSACTION: &str = "digest";

/// Default read mask for a checkpoint (`get_checkpoint`, `list_checkpoints`).
pub const CHECKPOINT: &str = "sequence_number,digest";

/// Default read mask for an event (`list_events`; there is no `get_events`).
pub const EVENT: &str = "event_type";

/// Default read mask for an epoch (`get_epoch`; there is no `list_epochs`).
pub const EPOCH: &str = "epoch,first_checkpoint,last_checkpoint,start,end,reference_gas_price,protocol_config.protocol_version";

/// Default read mask for an object (`get_object`, `list_object_versions`).
pub const OBJECT: &str = "object_id,version,digest";

/// Default `removal_read_mask` for each removal in an object's version listing
/// (`list_object_versions`).
pub const OBJECT_REMOVAL: &str = "version,kind";

/// Default read mask for an owned-object listing (`list_owned_objects`).
pub const OWNED_OBJECT: &str = "object_id,version,object_type";

/// Default read mask for a dynamic-field listing (`list_dynamic_fields`).
pub const DYNAMIC_FIELD: &str = "parent,field_id";

/// Default read mask for transaction execution (`ExecuteTransaction`).
pub const EXECUTE_TRANSACTION: &str = "effects";

/// Validate an optional request `read_mask` against message type `M`, falling
/// back to `default` when the request omits one, and return it as a
/// `FieldMaskTree`. Shared by every read-mask-bearing handler (get, list, and
/// subscribe) so validation and the "no mask -> default" rule stay identical
/// across them. `default` is an explicit argument because one message type can
/// carry different per-endpoint defaults (e.g. `ExecutedTransaction` uses
/// `TRANSACTION` for get/list but `EXECUTE_TRANSACTION` for execution).
pub fn validate_read_mask<M: MessageFields>(
    read_mask: Option<FieldMask>,
    default: &str,
) -> Result<FieldMaskTree, RpcError> {
    validate_field_mask::<M>("read_mask", read_mask, default)
}

/// [`validate_read_mask`] for a mask in request field `field`, which an invalid
/// path's `FieldViolation` names.
pub fn validate_field_mask<M: MessageFields>(
    field: &str,
    mask: Option<FieldMask>,
    default: &str,
) -> Result<FieldMaskTree, RpcError> {
    let mask = mask.unwrap_or_else(|| FieldMask::from_str(default));
    mask.validate::<M>().map_err(|path| {
        FieldViolation::new(field)
            .with_description(format!("invalid {field} path: {path}"))
            .with_reason(ErrorReason::FieldInvalid)
    })?;
    Ok(FieldMaskTree::from(mask))
}
