// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use sui_types::base_types::{ObjectID, ObjectRef};
use sui_types::digests::TransactionDigest;
use sui_types::error::{SuiErrorKind, SuiResult, UserInputError};
use sui_types::object::Object;
use sui_types::storage::ObjectStore;
use tracing::{debug, instrument};

use super::writeback_cache::WritebackCache;

pub(super) struct ObjectLocks {}

impl ObjectLocks {
    pub fn new() -> Self {
        Self {}
    }

    pub(crate) fn get_transaction_lock(
        &self,
        obj_ref: &ObjectRef,
        epoch_store: &AuthorityPerEpochStore,
    ) -> SuiResult<Option<TransactionDigest>> {
        epoch_store.tables()?.get_locked_transaction(obj_ref)
    }

    pub(crate) fn clear(&self) {
        // No-op: pre-consensus locking is disabled, so there's no in-memory lock state to clear.
        // Lock state is managed in the database via post-consensus locking.
    }

    fn verify_live_object(obj_ref: &ObjectRef, live_object: &Object) -> SuiResult {
        debug_assert_eq!(obj_ref.0, live_object.id());
        if obj_ref.1 != live_object.version() {
            debug!(
                "object version unavailable for consumption: {:?} (current: {})",
                obj_ref,
                live_object.version()
            );
            return Err(SuiErrorKind::UserInputError {
                error: UserInputError::ObjectVersionUnavailableForConsumption {
                    provided_obj_ref: *obj_ref,
                    current_version: live_object.version(),
                },
            }
            .into());
        }

        let live_digest = live_object.digest();
        if obj_ref.2 != live_digest {
            return Err(SuiErrorKind::UserInputError {
                error: UserInputError::InvalidObjectDigest {
                    object_id: obj_ref.0,
                    expected_digest: live_digest,
                },
            }
            .into());
        }

        Ok(())
    }

    fn multi_get_objects_must_exist(
        cache: &WritebackCache,
        object_ids: &[ObjectID],
    ) -> SuiResult<Vec<Object>> {
        let objects = cache.multi_get_objects(object_ids);
        let mut result = Vec::with_capacity(objects.len());
        for (i, object) in objects.into_iter().enumerate() {
            if let Some(object) = object {
                result.push(object);
            } else {
                return Err(SuiErrorKind::UserInputError {
                    error: UserInputError::ObjectNotFound {
                        object_id: object_ids[i],
                        version: None,
                    },
                }
                .into());
            }
        }
        Ok(result)
    }

    /// Validates owned object versions and digests without acquiring locks.
    /// Used to validate objects before signing, since locking happens post-consensus.
    #[instrument(level = "debug", skip_all)]
    pub(crate) fn validate_owned_object_versions(
        cache: &WritebackCache,
        owned_input_objects: &[ObjectRef],
    ) -> SuiResult {
        let object_ids = owned_input_objects.iter().map(|o| o.0).collect::<Vec<_>>();
        let live_objects = Self::multi_get_objects_must_exist(cache, &object_ids)?;

        // Validate that all objects are live and versions/digests match
        for (obj_ref, live_object) in owned_input_objects.iter().zip(live_objects.iter()) {
            Self::verify_live_object(obj_ref, live_object)?;
        }

        Ok(())
    }
}
