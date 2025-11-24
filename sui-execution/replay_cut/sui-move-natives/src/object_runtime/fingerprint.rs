// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::vm_status::StatusCode;
use move_vm_types::values::Value;
use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::{MoveObjectType, ObjectID};

/// This type is used to track if an object has changed since it was read from storage. Ideally,
/// this would just store the owner ID+type+BCS bytes of the object; however, due to pending
/// rewrites in the adapter, that would be too much code churn. Instead, we use the `Value` since
/// the `RuntimeResults` operate over `Value` and not BCS bytes.
pub struct ObjectFingerprint(Option<ObjectFingerprint_>);

enum ObjectFingerprint_ {
    /// The object did not exist (as a child object) in storage at the start of the transaction.
    Empty,
    // The object was loaded as a child object from storage.
    Preexisting {
        owner: ObjectID,
        ty: MoveObjectType,
        value: Value,
    },
}

impl ObjectFingerprint {
    #[cfg(debug_assertions)]
    pub fn is_disabled(&self) -> bool {
        self.0.is_none()
    }

    /// Creates a new object fingerprint for a child object not found in storage.
    /// Will be internally disabled if the feature is not enabled in the protocol config.
    pub fn none(protocol_config: &ProtocolConfig) -> Self {
        if !protocol_config.minimize_child_object_mutations() {
            Self(None)
        } else {
            Self(Some(ObjectFingerprint_::Empty))
        }
    }

    /// Creates a new object fingerprint for a child found in storage.
    /// Will be internally disabled if the feature is not enabled in the protocol config.
    pub fn preexisting(
        protocol_config: &ProtocolConfig,
        preexisting_owner: &ObjectID,
        preexisting_type: &MoveObjectType,
        preexisting_value: &Value,
    ) -> PartialVMResult<Self> {
        Ok(if !protocol_config.minimize_child_object_mutations() {
            Self(None)
        } else {
            Self(Some(ObjectFingerprint_::Preexisting {
                owner: *preexisting_owner,
                ty: preexisting_type.clone(),
                value: preexisting_value.copy_value()?,
            }))
        })
    }

    /// Checks if the object has changed since it was read from storage.
    /// Gives an invariant violation if the fingerprint is disabled.
    /// Gives an invariant violation if the values do not have the same layout, but the owner and
    /// type are thesame.
    pub fn object_has_changed(
        &self,
        final_owner: &ObjectID,
        final_type: &MoveObjectType,
        final_value: &Option<Value>,
    ) -> PartialVMResult<bool> {
        use ObjectFingerprint_ as F;
        let Some(inner) = &self.0 else {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR).with_message(
                    "Object fingerprint not enabled, yet we were asked for the changes".to_string(),
                ),
            );
        };
        Ok(match (inner, final_value) {
            (F::Empty, None) => false,
            (F::Empty, Some(_)) | (F::Preexisting { .. }, None) => true,
            (
                F::Preexisting {
                    owner: preexisting_owner,
                    ty: preexisting_type,
                    value: preexisting_value,
                },
                Some(final_value),
            ) => {
                // owner changed or value changed.
                // For the value, we must first check if the types are the same before comparing the
                // values
                !(preexisting_owner == final_owner
                    && preexisting_type == final_type
                    && preexisting_value.equals(final_value)?)
            }
        })
    }
}
