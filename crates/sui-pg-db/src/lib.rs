// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod db;
pub mod schema;
pub mod temp;

use diesel_migrations::{embed_migrations, EmbeddedMigrations};

// Re-export everything from db
pub use db::*;
pub use sui_field_count::FieldCount;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");
