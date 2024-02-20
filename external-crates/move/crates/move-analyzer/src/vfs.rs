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
use move_compiler::shared::{VFSFile, VFS};
use rand::Rng;
use std::{
    cmp, io,
    path::{Path, PathBuf},
    sync::Arc,
};

/// A mapping from identifiers (file names, potentially, but not necessarily) to their contents.
#[derive(Debug, Default, Clone)]

/// Virtual file system that serves the same version of the file each time it's queried,
/// whether this file comes from the IDE message (file open or update notification) or
/// it comes from the file system.
pub struct IDEVFS {
    /// Files pushed to the LSP server by the IDE via file open or update notifications
    /// (used concurrently hence using `CHashMap`)
    pub ide_files: Arc<CHashMap<PathBuf, Vec<u8>>>,
    /// Files served by this VFS (populated on demand from IDE files or from the file system)
    /// (used sequentially but need to read and write the map)
    pub all_files: Arc<CHashMap<PathBuf, Vec<u8>>>,
}

impl VFS for IDEVFS {
    // We may have a race here between a file being pushed by the IDE (and available in
    // `ide_files`) and files only available in the file system. This should be OK, though, as
    // in the worst case, we can always read from a file:
    // - if we attempt to get `ide_files` file but the window closes in the meantime and it's no
    // longer available, we still get up-to-date data from the file system (the file was saved
    // or not before window closing but it does not matter)
    // - if we attempt to read from file and the window opens in the meantime, the only
    // consequence is that we will temporarily build symbols for a slightly out-of-date data,
    // but this will quickly get updated once the user starts typing

    fn is_file(&self, fpath: &Path) -> bool {
        self.all_files.contains_key(fpath) || self.ide_files.contains_key(fpath) || fpath.is_file()
    }

    fn open_file(&self, fpath: &Path) -> io::Result<Box<dyn VFSFile>> {
        // a file entry has to be in `all_files` map before file representation is returned
        match self.all_files.get(fpath) {
            Some(_) => (),
            None => match self.ide_files.remove(fpath) {
                Some(s) => {
                    self.all_files.insert(fpath.to_path_buf(), s);
                }
                None => {
                    self.all_files
                        .insert(fpath.to_path_buf(), std::fs::read(fpath)?);
                }
            },
        }
        Ok(Box::new(IDEVFSFile {
            writeable: false,
            path: fpath.to_path_buf(),
            vfs: Arc::new(self.clone()),
        }))
    }

    fn create_tmp_file_in(&self, fpath: &Path) -> io::Result<Box<dyn VFSFile>> {
        let tmp_path = fpath.join(&rand::thread_rng().gen::<u64>().to_string());
        if self.all_files.contains_key(&tmp_path) || self.ide_files.contains_key(&tmp_path) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("temp file at {:?} alrady exists", tmp_path),
            ));
        }
        self.all_files.insert(tmp_path.clone(), vec![]);
        Ok(Box::new(IDEVFSFile {
            writeable: true,
            path: tmp_path,
            vfs: Arc::new(self.clone()),
        }))
    }

    fn create_dir_all(&self, _fpath: &Path) -> io::Result<()> {
        // do nothing as writes do not leave a mark in the actual file system
        Ok(())
    }
}

pub struct IDEVFSFile {
    /// Opened for write (as well as read)?
    writeable: bool,
    path: PathBuf,
    vfs: Arc<IDEVFS>,
}

impl VFSFile for IDEVFSFile {
    fn read_to_string(&mut self, buf: &mut String) -> io::Result<usize> {
        let Some(bytes) = self.vfs.all_files.get(&self.path) else {
            // IDEVSFile instance is created only after its entry in `all_files`
            // is inserted, but it might have been renamed
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("file at {:?} alrady exists", self.path),
            ));
        };
        buf.push_str(std::str::from_utf8(&bytes).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "file did not contain valid UTF-8",
            )
        })?);
        Ok(bytes.len())
    }

    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        let Some(bytes) = self.vfs.all_files.get(&self.path) else {
            // IDEVSFile instance is created only after its entry in `all_files`
            // is inserted, but it might have been renamed
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("file at {:?} alrady exists", self.path),
            ));
        };
        buf.clear();
        buf.extend(bytes.clone());
        Ok(bytes.len())
    }

    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let Some(bytes) = self.vfs.all_files.get(&self.path) else {
            // IDEVSFile instance is created only after its entry in `all_files`
            // is inserted, but it might have been renamed
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("file at {:?} alrady exists", self.path),
            ));
        };
        let to_read = cmp::min(bytes.len(), buf.len());
        buf.copy_from_slice(&bytes[0..to_read]);
        Ok(to_read)
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        if !self.writeable {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "file was not writeable",
            ));
        }
        let Some(mut bytes) = self.vfs.all_files.get_mut(&self.path) else {
            // IDEVSFile instance is created only after its entry in `all_files`
            // is inserted, but it might have been renamed
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("file at {:?} alrady exists", self.path),
            ));
        };
        bytes.clear();
        bytes.extend(buf);
        Ok(())
    }

    fn rename(&mut self, dst_path: &Path) -> io::Result<()> {
        let Some(bytes) = self.vfs.all_files.remove(&self.path) else {
            // IDEVSFile instance is created only after its entry in `all_files`
            // is inserted, but it might have been renamed
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("file at {:?} alrady exists", self.path),
            ));
        };
        self.vfs.all_files.insert(dst_path.to_path_buf(), bytes);
        self.path = dst_path.to_path_buf();
        Ok(())
    }
}

/// Updates the given virtual file system based on the text document sync notification that was sent.
pub fn on_text_document_sync_notification(
    files: Arc<CHashMap<PathBuf, Vec<u8>>>,
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
                parameters.text_document.text.into_bytes(),
            );
            symbolicator_runner.run(parameters.text_document.uri.to_file_path().unwrap());
        }
        lsp_types::notification::DidChangeTextDocument::METHOD => {
            let mut parameters =
                serde_json::from_value::<DidChangeTextDocumentParams>(notification.params.clone())
                    .expect("could not deserialize notification");
            files.insert(
                parameters.text_document.uri.to_file_path().unwrap(),
                parameters.content_changes.pop().unwrap().text.into_bytes(),
            );
            symbolicator_runner.run(parameters.text_document.uri.to_file_path().unwrap());
        }
        lsp_types::notification::DidSaveTextDocument::METHOD => {
            let parameters =
                serde_json::from_value::<DidSaveTextDocumentParams>(notification.params.clone())
                    .expect("could not deserialize notification");
            files.insert(
                parameters.text_document.uri.to_file_path().unwrap(),
                parameters.text.unwrap().into_bytes(),
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
