// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use sui_types::effects::{IDOperation, ObjectChange as NativeObjectChange};

use super::{object::Object, sui_address::SuiAddress};

pub(crate) struct ObjectChange {
    pub native: NativeObjectChange,
    /// The checkpoint sequence number this was viewed at.
    pub checkpoint_viewed_at: u64,
}

/// Effect on an individual Object (keyed by its ID).
#[Object]
impl ObjectChange {
    /// The address of the object that has changed.
    async fn address(&self) -> SuiAddress {
        self.native.id.into()
    }

    /// The contents of the object immediately before the transaction.
    async fn input_state(&self, ctx: &Context<'_>) -> Result<Option<Object>> {
        let Some(version) = self.native.input_version else {
            return Ok(None);
        };

        Object::query(
            ctx,
            self.native.id.into(),
            Object::at_version(version.value(), self.checkpoint_viewed_at),
        )
        .await
        .extend()
    }

    /// The contents of the object immediately after the transaction.
    async fn output_state(&self, ctx: &Context<'_>) -> Result<Option<Object>> {
        let Some(version) = self.native.output_version else {
            return Ok(None);
        };

        Object::query(
            ctx,
            self.native.id.into(),
            Object::at_version(version.value(), self.checkpoint_viewed_at),
        )
        .await
        .extend()
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
