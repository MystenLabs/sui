// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! The language server must operate upon Move source buffers as they are being edited.
//! As a result, it is frequently queried about buffers that have not yet (or may never be) saved
//! to the actual file system.
//!
//! To manage these buffers, this module provides a "virtual file system" -- in reality, it is
//! basically just a mapping from file identifier (this could be the file's path were it to be
//! saved) to its textual contents.

use crate::symbols;
use chashmap::CHashMap;
use lsp_server::Notification;
use lsp_types::{
    notification::Notification as _, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams,
};
use move_compiler::shared::VFS;
use std::{
    collections::HashMap,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

/// A mapping from identifiers (file names, potentially, but not necessarily) to their contents.
#[derive(Debug, Default, Clone)]

/// Virtual file system that serves the same version of the file each time it's queried,
/// whether this file comes from the IDE message (file open or update notification) or
/// it comes from the file system.
pub struct VirtualFileSystem {
    /// Files pushed to the LSP server by the IDE via file open or update notifications
    pub ide_files: Arc<CHashMap<PathBuf, String>>,
    /// Files served by this VFS (populated on demand from IDE files or from the file system)
    pub all_files: HashMap<PathBuf, String>,
}

impl VFS for VirtualFileSystem {
    fn read_to_string(&mut self, fpath: &Path, buf: &mut String) -> std::io::Result<usize> {
        // We may have a race here between a file being pushed by the IDE (and available in
        // `ide_files`) and files only available in the file system. This should be OK, though, as
        // in the worst case, we can always read from a file:
        // - if we attempt to get `ide_files` file but the window closes in the meantime and it's no
        // longer available, we still get up-to-date data from the file system (the file was saved
        // or not before window closing but it does not matter)
        // - if we attempt to read from file and the window opens in the meantime, the only
        // consequence is that we will temporarily build symbols for a slightly out-of-date data,
        // but this will quickly get updated once the user starts typing
        match self.all_files.get(fpath) {
            Some(s) => {
                buf.push_str(s.as_str());
                Ok(s.len())
            }
            None => match self.ide_files.remove(fpath) {
                Some(s) => {
                    buf.push_str(s.as_str());
                    let len = s.len();
                    self.all_files.insert(fpath.to_path_buf(), s);
                    Ok(len)
                }
                None => {
                    let mut f = std::fs::File::open(fpath).map_err(|err| {
                        std::io::Error::new(err.kind(), format!("{}: {:?}", err, fpath))
                    })?;
                    let len = f.read_to_string(buf)?;
                    self.all_files.insert(fpath.to_path_buf(), buf.clone());
                    Ok(len)
                }
            },
        }
    }
}

/// Updates the given virtual file system based on the text document sync notification that was sent.
pub fn on_text_document_sync_notification(
    files: Arc<CHashMap<PathBuf, String>>,
    symbolicator_runner: &symbols::SymbolicatorRunner,
    notification: &Notification,
) {
    // TODO: A far more efficient "virtual file system" would update its buffers with changes sent
    // from the client, instead of completely replacing them each time. The rust-analyzer has a
    // 'vfs' module that is capable of doing just that, but it is not published on crates.io. If
    // we could help get it published, we could use it here.
    eprintln!("text document notification");
    match notification.method.as_str() {
        lsp_types::notification::DidOpenTextDocument::METHOD => {
            let parameters =
                serde_json::from_value::<DidOpenTextDocumentParams>(notification.params.clone())
                    .expect("could not deserialize notification");
            files.insert(
                parameters.text_document.uri.to_file_path().unwrap(),
                parameters.text_document.text,
            );
            symbolicator_runner.run(parameters.text_document.uri.to_file_path().unwrap());
        }
        lsp_types::notification::DidChangeTextDocument::METHOD => {
            let mut parameters =
                serde_json::from_value::<DidChangeTextDocumentParams>(notification.params.clone())
                    .expect("could not deserialize notification");
            files.insert(
                parameters.text_document.uri.to_file_path().unwrap(),
                parameters.content_changes.pop().unwrap().text,
            );
            symbolicator_runner.run(parameters.text_document.uri.to_file_path().unwrap());
        }
        lsp_types::notification::DidSaveTextDocument::METHOD => {
            let parameters =
                serde_json::from_value::<DidSaveTextDocumentParams>(notification.params.clone())
                    .expect("could not deserialize notification");
            files.insert(
                parameters.text_document.uri.to_file_path().unwrap(),
                parameters.text.unwrap(),
            );
            symbolicator_runner.run(parameters.text_document.uri.to_file_path().unwrap());
        }
        lsp_types::notification::DidCloseTextDocument::METHOD => {
            let parameters =
                serde_json::from_value::<DidCloseTextDocumentParams>(notification.params.clone())
                    .expect("could not deserialize notification");
            files.remove(&parameters.text_document.uri.to_file_path().unwrap());
        }
        _ => eprintln!("invalid notification '{}'", notification.method),
    }
    eprintln!("text document notification handled");
}
