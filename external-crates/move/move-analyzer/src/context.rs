// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{symbols::Symbols, vfs::VirtualFileSystem};
use lsp_server::Connection;
use std::sync::{Arc, Mutex};

/// The context within which the language server is running.
pub struct Context {
    /// The connection with the language server's client.
    pub connection: Connection,
    /// The files that the language server is providing information about.
    pub files: VirtualFileSystem,
    /// Symbolication information
    pub symbols: Arc<Mutex<Symbols>>,
}
