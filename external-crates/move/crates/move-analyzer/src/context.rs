// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::symbols::Symbols;
use lsp_server::Connection;
use std::sync::{Arc, Mutex};

/// The context within which the language server is running.
pub struct Context {
    /// The connection with the language server's client.
    pub connection: Connection,
    /// Symbolication information
    pub symbols: Arc<Mutex<Symbols>>,
    /// Are inlay type hints enabled?
    pub inlay_type_hints: bool,
}
