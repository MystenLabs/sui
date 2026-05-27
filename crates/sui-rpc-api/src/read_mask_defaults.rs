// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Default read mask for a transaction (`get_transaction`, `list_transactions`).
pub const TRANSACTION: &str = "digest";

/// Default read mask for a checkpoint (`get_checkpoint`, `list_checkpoints`).
pub const CHECKPOINT: &str = "sequence_number,digest";

/// Default read mask for an event (`list_events`; there is no `get_events`).
pub const EVENT: &str = "event_type";

/// Default read mask for an epoch (`get_epoch`; there is no `list_epochs`).
pub const EPOCH: &str = "epoch,first_checkpoint,last_checkpoint,start,end,reference_gas_price,protocol_config.protocol_version";

/// Default read mask for an object (`get_object`).
pub const OBJECT: &str = "object_id,version,digest";

/// Default read mask for an owned-object listing (`list_owned_objects`).
pub const OWNED_OBJECT: &str = "object_id,version,object_type";

/// Default read mask for a dynamic-field listing (`list_dynamic_fields`).
pub const DYNAMIC_FIELD: &str = "parent,field_id";

/// Default read mask for transaction execution (`ExecuteTransaction`).
pub const EXECUTE_TRANSACTION: &str = "effects";
