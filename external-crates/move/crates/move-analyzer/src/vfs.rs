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
use lsp_server::Notification;
use lsp_types::{
    notification::Notification as _, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams,
};
use std::{io::Write, path::PathBuf};
use vfs::VfsPath;

/// A mapping from identifiers (file names, potentially, but not necessarily) to their contents.
#[derive(Debug, Default)]
pub struct VirtualFileSystem {
    files: std::collections::HashMap<PathBuf, String>,
}

impl VirtualFileSystem {
    /// Returns a reference to the buffer corresponding to the given identifier, or `None` if it
    /// is not present in the system.
    pub fn get(&self, identifier: &PathBuf) -> Option<&str> {
        self.files.get(identifier).map(|s| s.as_str())
    }

    /// Inserts or overwrites the buffer corresponding to the given identifier.
    ///
    /// TODO: A far more efficient "virtual file system" would update its buffers with changes sent
    /// from the client, instead of completely replacing them each time. The rust-analyzer has a
    /// 'vfs' module that is capable of doing just that, but it is not published on crates.io. If
    /// we could help get it published, we could use it here.
    pub fn update(&mut self, identifier: PathBuf, content: &str) {
        self.files.insert(identifier, content.to_string());
    }

    /// Removes the buffer and its identifier from the system.
    pub fn remove(&mut self, identifier: &PathBuf) {
        self.files.remove(identifier);
    }
}

/// Updates the given virtual file system based on the text document sync notification that was sent.
pub fn on_text_document_sync_notification(
    ide_files_root: VfsPath,
    symbolicator_runner: &symbols::SymbolicatorRunner,
    notification: &Notification,
) {
    fn vfs_file_create(
        ide_files: &VfsPath,
        file_path: PathBuf,
        first_access: bool,
    ) -> Option<Box<dyn Write + Send>> {
        let Some(vfs_path) = ide_files.join(file_path.to_string_lossy()).ok() else {
            eprintln!(
                "Could not construct file path for file creation at {:?}",
                file_path
            );
            return None;
        };
        if first_access {
            // create all directories on first access, otherwise file creation will fail
            let _ = vfs_path.parent().create_dir_all();
        }
        let Some(vfs_file) = vfs_path.create_file().ok() else {
            eprintln!("Could not create file at {:?}", vfs_path);
            return None;
        };
        Some(vfs_file)
    }

    fn vfs_file_remove(ide_files: &VfsPath, file_path: PathBuf) {
        let Some(vfs_path) = ide_files.join(file_path.to_string_lossy()).ok() else {
            eprintln!(
                "Could not construct file path for file removal at {:?}",
                file_path
            );
            return;
        };
        if vfs_path.remove_file().is_err() {
            eprintln!("Could not remove file at {:?}", vfs_path);
        };
    }

    eprintln!("text document notification");
    match notification.method.as_str() {
        lsp_types::notification::DidOpenTextDocument::METHOD => {
            let parameters =
                serde_json::from_value::<DidOpenTextDocumentParams>(notification.params.clone())
                    .expect("could not deserialize notification");
            let Some(file_path) = parameters.text_document.uri.to_file_path().ok() else {
                eprintln!(
                    "Could not create file path from URI {:?}",
                    parameters.text_document.uri
                );
                return;
            };
            let Some(mut vfs_file) = vfs_file_create(
                &ide_files_root,
                file_path.clone(),
                /* first_access */ true,
            ) else {
                return;
            };
            if vfs_file
                .write_all(parameters.text_document.text.as_bytes())
                .is_ok()
            {
                symbolicator_runner.run(file_path);
            }
        }
        lsp_types::notification::DidChangeTextDocument::METHOD => {
            let parameters =
                serde_json::from_value::<DidChangeTextDocumentParams>(notification.params.clone())
                    .expect("could not deserialize notification");

            let Some(file_path) = parameters.text_document.uri.to_file_path().ok() else {
                eprintln!(
                    "Could not create file path from URI {:?}",
                    parameters.text_document.uri
                );
                return;
            };
            let Some(mut vfs_file) = vfs_file_create(
                &ide_files_root,
                file_path.clone(),
                /* first_access */ false,
            ) else {
                return;
            };
            let Some(changes) = parameters.content_changes.last() else {
                eprintln!("Could not read last opened file change");
                return;
            };
            if vfs_file.write_all(changes.text.as_bytes()).is_ok() {
                symbolicator_runner.run(file_path);
            }
        }
        lsp_types::notification::DidSaveTextDocument::METHOD => {
            let parameters =
                serde_json::from_value::<DidSaveTextDocumentParams>(notification.params.clone())
                    .expect("could not deserialize notification");
            let Some(file_path) = parameters.text_document.uri.to_file_path().ok() else {
                eprintln!(
                    "Could not create file path from URI {:?}",
                    parameters.text_document.uri
                );
                return;
            };
            let Some(mut vfs_file) = vfs_file_create(
                &ide_files_root,
                file_path.clone(),
                /* first_access */ false,
            ) else {
                return;
            };
            let Some(content) = parameters.text else {
                eprintln!("Could not read saved file change");
                return;
            };
            if vfs_file.write_all(content.as_bytes()).is_err() {
                // try to remove file from the file system and schedule symbolicator to pick up
                // changes from the file system
                vfs_file_remove(&ide_files_root, file_path.clone());
                symbolicator_runner.run(file_path);
            }
        }
        lsp_types::notification::DidCloseTextDocument::METHOD => {
            let parameters =
                serde_json::from_value::<DidCloseTextDocumentParams>(notification.params.clone())
                    .expect("could not deserialize notification");
            let Some(file_path) = parameters.text_document.uri.to_file_path().ok() else {
                eprintln!(
                    "Could not create file path from URI {:?}",
                    parameters.text_document.uri
                );
                return;
            };
            vfs_file_remove(&ide_files_root, file_path.clone());
        }
        _ => eprintln!("invalid notification '{}'", notification.method),
    }
    eprintln!("text document notification handled");
}
