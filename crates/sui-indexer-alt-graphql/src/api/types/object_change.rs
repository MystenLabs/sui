// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Object;
use sui_types::effects::{IDOperation, ObjectChange as NativeObjectChange};

use crate::{api::scalars::sui_address::SuiAddress, scope::Scope};

use super::object::Object;

pub(crate) struct ObjectChange<'a> {
    pub(crate) scope: Scope,
    pub(crate) native: NativeObjectChange,
    pub(crate) input_object: Option<&'a sui_types::object::Object>,
    pub(crate) output_object: Option<&'a sui_types::object::Object>,
}

#[Object]
impl ObjectChange<'_> {
    /// The address of the object that has changed.
    async fn address(&self) -> SuiAddress {
        self.native.id.into()
    }

    /// The contents of the object immediately before the transaction.
    async fn input_state(&self) -> Option<Object> {
        // Use execution data if available
        if let Some(input_obj) = self.input_object {
            return Some(Object::from_contents(
                self.scope.clone(),
                input_obj.clone(),
            ));
        }

        let NativeObjectChange {
            id,
            input_version: Some(version),
            input_digest: Some(digest),
            ..
        } = self.native
        else {
            return None;
        };

        Some(Object::with_ref(&self.scope, id.into(), version, digest))
    }

    /// The contents of the object immediately after the transaction.
    async fn output_state(&self) -> Option<Object> {
        // Use execution data if available
        if let Some(output_obj) = self.output_object {
            return Some(Object::from_contents(
                self.scope.clone(),
                output_obj.clone(),
            ));
        }

        let NativeObjectChange {
            id,
            output_version: Some(version),
            output_digest: Some(digest),
            ..
        } = self.native
        else {
            return None;
        };

        Some(Object::with_ref(&self.scope, id.into(), version, digest))
    }

    /// Whether the ID was created in this transaction.
    async fn id_created(&self) -> Option<bool> {
        Some(self.native.id_operation == IDOperation::Created)
    }

    /// Whether the ID was deleted in this transaction.
    async fn id_deleted(&self) -> Option<bool> {
        Some(self.native.id_operation == IDOperation::Deleted)
    }
}
