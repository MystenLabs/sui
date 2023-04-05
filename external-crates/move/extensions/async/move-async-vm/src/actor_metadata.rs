// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{
    identifier::{IdentStr, Identifier},
    language_storage::{ModuleId, StructTag},
};
use sha3::{Digest, Sha3_256};
use std::convert::TryInto;

/// Metadata for an actor.
/// TODO: we want to attach this to file format at some point.
#[derive(Clone, Debug)]
pub struct ActorMetadata {
    pub module_id: ModuleId,
    pub state_tag: StructTag,
    pub initializer: Identifier,
    pub messages: Vec<Identifier>,
}

/// Compute a hash for a message.
pub fn message_hash(module_id: &ModuleId, handler_id: &IdentStr) -> u64 {
    let hash_str = format!(
        "0x{}::{}::{}",
        module_id.address(),
        module_id.name(),
        handler_id
    );
    let hash_bytes: [u8; 8] = Sha3_256::digest(hash_str.as_bytes())[0..8]
        .try_into()
        .unwrap();
    u64::from_be_bytes(hash_bytes)
}
