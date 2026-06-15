// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! [`ChildObjectResolver`] adapter.
//!
//! `ChildObjectResolver` is one of the supertrait bounds on
//! [`sui_types::storage::RpcStateReader`]. Its methods feed the
//! Move runtime's dynamic-field / receive-object paths. **This
//! adapter is read-only and does not serve Move execution**: it
//! returns `Ok(None)` from both methods.
//!
//! Callers that need execution-time child-object resolution
//! (re-running a transaction, simulating dry-runs) should keep
//! using the validator perpetual store — this adapter is meant
//! for the read-only RPC surface where child-object lookups never
//! arise.

use sui_consistent_store::reader::Reader;
use sui_types::base_types::EpochId;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::error::SuiResult;
use sui_types::object::Object;
use sui_types::storage::ChildObjectResolver;

use crate::reader::RpcStoreReader;

impl<R: Reader + Send + Sync> ChildObjectResolver for RpcStoreReader<R> {
    fn read_child_object(
        &self,
        _parent: &ObjectID,
        _child: &ObjectID,
        _child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        Ok(None)
    }

    fn get_object_received_at_version(
        &self,
        _owner: &ObjectID,
        _receiving_object_id: &ObjectID,
        _receive_object_at_version: SequenceNumber,
        _epoch_id: EpochId,
    ) -> SuiResult<Option<Object>> {
        Ok(None)
    }
}
