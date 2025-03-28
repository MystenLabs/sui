// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod store;

#[cfg(feature = "postgres")]
pub mod schema;

#[cfg(feature = "postgres")]
pub mod pg_store;

#[cfg(feature = "postgres")]
use diesel_migrations::{embed_migrations, EmbeddedMigrations};
#[cfg(feature = "postgres")]
pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");
