// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

use diesel::{ExpressionMethods, OptionalExtension, QueryDsl};
use diesel_async::scoped_futures::ScopedFutureExt;
use move_core_types::annotated_value as A;
use sui_display::v1::Format;
use sui_indexer::{models::display::StoredDisplay, schema::display};
use sui_types::TypeTag;

use crate::{
    data::{Db, DbConnection, QueryExecutor},
    error::Error,
};

pub(crate) struct Display {
    pub stored: StoredDisplay,
}

/// Maximum depth of nested fields.
const MAX_DEPTH: usize = 10;

/// Maximum size of Display output.
const MAX_OUTPUT_SIZE: usize = 1024 * 1024;

/// The set of named templates defined on-chain for the type of this object,
/// to be handled off-chain. The server substitutes data from the object
/// into these templates to generate a display string per template.
#[derive(Debug, SimpleObject)]
pub(crate) struct DisplayEntry {
    /// The identifier for a particular template string of the Display object.
    pub key: String,
    /// The template string for the key with placeholder values substituted.
    pub value: Option<String>,
    /// An error string describing why the template could not be rendered.
    pub error: Option<String>,
}

impl Display {
    /// Query for a `Display` object by the type that it is displaying
    pub(crate) async fn query(db: &Db, type_: TypeTag) -> Result<Option<Display>, Error> {
        let stored: Option<StoredDisplay> = db
            .execute(move |conn| {
                async move {
                    conn.first(move || {
                        use display::dsl;
                        dsl::display.filter(
                            dsl::object_type.eq(type_.to_canonical_string(/* with_prefix */ true)),
                        )
                    })
                    .await
                    .optional()
                }
                .scope_boxed()
            })
            .await?;

        Ok(stored.map(|stored| Display { stored }))
    }

    /// Render the fields defined by this `Display` from the contents of `struct_`.
    pub(crate) fn render(
        &self,
        bytes: &[u8],
        layout: &A::MoveTypeLayout,
    ) -> Result<Vec<DisplayEntry>, Error> {
        let fields = self
            .stored
            .to_display_update_event()
            .map_err(|e| Error::Internal(e.to_string()))?
            .fields;

        let mut rendered = vec![];
        for (field, value) in Format::parse(MAX_DEPTH, &fields)
            .map_err(|e| Error::Client(e.to_string()))?
            .display(MAX_OUTPUT_SIZE, bytes, layout)
            .map_err(|e| Error::Client(e.to_string()))?
        {
            rendered.push(match value {
                Ok(v) => DisplayEntry::create_value(field, v),
                Err(e) => DisplayEntry::create_error(field, e.to_string()),
            });
        }

        Ok(rendered)
    }
}

impl DisplayEntry {
    pub(crate) fn create_value(key: String, value: String) -> Self {
        Self {
            key,
            value: Some(value),
            error: None,
        }
    }

    pub(crate) fn create_error(key: String, error: String) -> Self {
        Self {
            key,
            value: None,
            error: Some(error),
        }
    }
}
