// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::db_shell::vfs::VfsPath;

pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
}

pub trait Backend: Send + Sync {
    /// List the children of `path` (normal directory listing).
    fn ls_children(&self, path: &VfsPath, limit: usize) -> anyhow::Result<Vec<DirEntry>>;

    /// List starting from `path` as a cursor into the parent namespace.
    /// Used for `ls /checkpoints/seq/1234` which means "list 30 from seq 1234".
    fn ls_cursor(&self, path: &VfsPath, limit: usize) -> anyhow::Result<Vec<DirEntry>>;

    /// Return the JSON representation of the entity at `path`.
    fn read_json(&self, path: &VfsPath) -> anyhow::Result<serde_json::Value>;

    /// Return the Rust `{:#?}` debug representation of the entity at `path`.
    fn read_debug(&self, path: &VfsPath) -> anyhow::Result<String>;

    /// Return the raw BCS bytes of the entity at `path`.
    fn read_bcs(&self, path: &VfsPath) -> anyhow::Result<Vec<u8>>;

    /// Delete the entity at `path`. Destructive and permanent.
    fn delete(&self, path: &VfsPath) -> anyhow::Result<()>;
}
