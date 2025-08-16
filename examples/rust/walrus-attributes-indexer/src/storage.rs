// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::Insertable;
use sui_indexer_alt_framework::FieldCount;

use crate::schema::blog_post;

// ============================================================================
// DATABASE STORAGE TYPES
// ============================================================================
// These types represent the structure of data as it's stored in the database.
// They map directly to database tables and include Diesel annotations.

/// Representation of a row from the `blog_post` table, which maps a blob to its associated Sui Blob
/// object and the latest dynamic field metadata for traceability.
#[derive(Insertable, Debug, FieldCount, Clone)]
#[diesel(table_name = blog_post)]
pub struct StoredBlogPost {
    /// The ID of the Metadata dynamic field.
    pub dynamic_field_id: Vec<u8>,
    /// The version of the Metadata dynamic field.
    pub df_version: i64,
    /// Address that published the Walrus Blob.
    pub publisher: Vec<u8>,
    /// The Blob ID to be used to fetch the Walrus blob. This can be selected in postgres with:
    ///
    /// SELECT replace(replace(rtrim(encode(blob_id, 'base64'), '='), '+', '-'), '/', '_') as
    /// blob_id FROM walrus_blob;
    pub blob_id: String,
    /// Metadata content, the count of views.
    pub view_count: i64,
    /// Metadata content, the title of the blog post.
    pub title: String,
}
