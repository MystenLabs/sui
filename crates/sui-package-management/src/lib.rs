// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::base_types::ObjectID;

pub mod system_package_versions;

/// TODO(pkg-alt): Move this to a crate we really want to use.
pub enum LockCommand {
    Publish,
    Upgrade,
}

/// TODO(pkg-alt): Remove these once we figure out deps.
#[derive(thiserror::Error, Debug, Clone)]
pub enum PublishedAtError {
    #[error("The 'published-at' field in Move.toml or Move.lock is invalid: {0:?}")]
    Invalid(String),

    #[error("The 'published-at' field is not present in Move.toml or Move.lock")]
    NotPresent,

    #[error(
        "Conflicting 'published-at' addresses between Move.toml -- {id_manifest} -- and \
         Move.lock -- {id_lock}"
    )]
    Conflict {
        id_lock: ObjectID,
        id_manifest: ObjectID,
    },
}
