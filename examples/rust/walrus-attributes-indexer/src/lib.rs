// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel_migrations::{embed_migrations, EmbeddedMigrations};

mod schema;
mod storage;
mod types;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

pub mod handlers;
