// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use sui_indexer::types_v2::IndexedObjectChange;

use super::{object::Object, sui_address::SuiAddress};
use crate::{context_data::db_data_provider::PgManager, error::Error};

pub(crate) struct ObjectChange {
    // TODO: input_key (waiting for object history)
    output_key: Option<(SuiAddress, u64)>,
    id_created: bool,
    id_deleted: bool,
}

/// Effect on an individual Object (keyed by its ID).
#[Object]
impl ObjectChange {
    /// The contents of the object at the end of the transaction.
    async fn output_state(&self, ctx: &Context<'_>) -> Result<Option<Object>> {
        let Some((id, version)) = self.output_key else {
            return Ok(None);
        };

        ctx.data_unchecked::<PgManager>()
            .fetch_obj(id, Some(version))
            .await
            .extend()
    }

    /// Whether the ID was created in this transaction.
    async fn id_created(&self) -> Option<bool> {
        Some(self.id_created)
    }

    /// Whether the ID was deleted in this transaction.
    async fn id_deleted(&self) -> Option<bool> {
        Some(self.id_deleted)
    }
}

impl ObjectChange {
    pub(crate) fn read(bytes: &[u8]) -> Result<Self, Error> {
        use IndexedObjectChange as O;

        let stored: O = bcs::from_bytes(bytes)
            .map_err(|e| Error::Internal(format!("Cannot deserialize ObjectChange: {e}")))?;

        Ok(match stored {
            O::Published {
                package_id: object_id,
                version,
                ..
            }
            | O::Created {
                object_id, version, ..
            } => ObjectChange {
                output_key: Some((object_id.into(), version.value())),
                id_created: true,
                id_deleted: false,
            },

            O::Transferred {
                object_id, version, ..
            }
            | O::Mutated {
                object_id, version, ..
            } => ObjectChange {
                output_key: Some((object_id.into(), version.value())),
                id_created: false,
                id_deleted: false,
            },

            O::Deleted { .. } => ObjectChange {
                output_key: None,
                id_created: false,
                id_deleted: true,
            },

            O::Wrapped { .. } => ObjectChange {
                output_key: None,
                id_created: false,
                id_deleted: false,
            },
        })
    }
}
