// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use codespan_reporting::files::{Files, SimpleFiles};
use move_command_line_common::files::FileHash;
use move_ir_types::location::*;
use move_symbol_pool::Symbol;
use std::{
    collections::HashMap,
    sync::Arc,
};

//**************************************************************************************************
// Types
//**************************************************************************************************

pub type FileId = usize;
pub type FileName = Symbol;

pub type FilesSourceText = HashMap<FileHash, (FileName, Arc<str>)>;

/// A mapping from file ids to file contents along with the mapping of filehash to fileID.
pub struct MappedFiles {
    pub files: SimpleFiles<Symbol, Arc<str>>,
    pub file_mapping: HashMap<FileHash, FileId>,
}

/// A file, and the line:column start, and line:column end that corresponds to a `Loc`
#[allow(dead_code)]
pub struct FilePosition {
    pub file_id: FileId,
    pub start: Position,
    pub end: Position,
}

/// A position holds the byte offset along with the line and column location in a file.
/// Both are zero-indexed.
pub struct Position {
    pub line: usize,
    pub column: usize,
    pub byte: usize,
}

/// A file, and the usize start and usize end that corresponds to a `Loc`
pub struct FileByteSpan {
    pub file_id: FileId,
    pub byte_span: ByteSpan,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ByteSpan {
    pub start: usize,
    pub end: usize,
}

//**************************************************************************************************
// Traits and Impls
//**************************************************************************************************

impl MappedFiles {
    pub fn new(files: FilesSourceText) -> Self {
        let mut simple_files = SimpleFiles::new();
        let mut file_mapping = HashMap::new();
        for (fhash, (fname, source)) in files {
            let id = simple_files.add(fname, source);
            file_mapping.insert(fhash, id);
        }
        Self {
            files: simple_files,
            file_mapping,
        }
    }

    pub fn empty() -> Self {
        Self {
            files: SimpleFiles::new(),
            file_mapping: HashMap::new(),
        }
    }

    pub fn add(&mut self, fhash: FileHash, fname: FileName, source: Arc<str>) {
        let id = self.files.add(fname, source);
        self.file_mapping.insert(fhash, id);
    }

    pub fn files(&self) -> &SimpleFiles<Symbol, Arc<str>> {
        &self.files
    }

    pub fn file_mapping(&self) -> &HashMap<FileHash, FileId> {
        &self.file_mapping
    }

    pub fn filename(&self, fhash: &FileHash) -> &str {
        let file_id = self.file_mapping().get(fhash).unwrap();
        self.files().get(*file_id).unwrap().name()
    }

    pub fn file_hash_to_file_id(&self, fhash: &FileHash) -> Option<FileId> {
        self.file_mapping().get(fhash).copied()
    }

    /// Like `start_position_opt`, but may panic
    pub fn start_position(&self, loc: &Loc) -> Position {
        self.position(loc).start
    }

    /// Like `end_position_opt`, but may panic
    pub fn end_position(&self, loc: &Loc) -> Position {
        self.position(loc).end
    }

    /// Like `position_opt`, but may panic
    pub fn position(&self, loc: &Loc) -> FilePosition {
        self.position_opt(loc).unwrap()
    }

    /// Like `byte_span_opt`, but may panic
    pub fn byte_span(&self, loc: &Loc) -> FileByteSpan {
        self.byte_span_opt(loc).unwrap()
    }

    pub fn start_position_opt(&self, loc: &Loc) -> Option<Position> {
        self.position_opt(loc).map(|posn| posn.start)
    }

    pub fn end_position_opt(&self, loc: &Loc) -> Option<Position> {
        self.position_opt(loc).map(|posn| posn.end)
    }

    pub fn position_opt(&self, loc: &Loc) -> Option<FilePosition> {
        let start_loc = loc.start() as usize;
        let end_loc = loc.end() as usize;
        let file_id = *self.file_mapping().get(&loc.file_hash())?;
        let start_file_loc = self.files().location(file_id, start_loc).ok()?;
        let end_file_loc = self.files().location(file_id, end_loc).ok()?;
        let posn = FilePosition {
            file_id,
            start: Position {
                line: start_file_loc.line_number - 1,
                column: start_file_loc.column_number - 1,
                byte: start_loc,
            },
            end: Position {
                line: end_file_loc.line_number - 1,
                column: end_file_loc.column_number - 1,
                byte: end_loc,
            },
        };
        Some(posn)
    }

    pub fn byte_span_opt(&self, loc: &Loc) -> Option<FileByteSpan> {
        let start = loc.start() as usize;
        let end = loc.end() as usize;
        let file_id = *self.file_mapping().get(&loc.file_hash())?;
        let posn = FileByteSpan {
            byte_span: ByteSpan { start, end },
            file_id,
        };
        Some(posn)
    }

    /// Given a line number in the file return the `Loc` for the line.
    fn line_to_loc_opt(&self, file_hash: &FileHash, line_number: usize) -> Option<Loc> {
        let file_id = self.file_mapping().get(file_hash)?;
        let line_range = self.files().line_range(*file_id, line_number).ok()?;
        Some(Loc::new(
            *file_hash,
            line_range.start as u32,
            line_range.end as u32,
        ))
    }

    /// Given a location `Loc` return a new loc only for source with leading and trailing
    /// whitespace removed.
    fn trimmed_loc_opt(&self, loc: &Loc) -> Option<Loc> {
        let source_str = self.source_of_loc_opt(loc)?;
        let trimmed_front = source_str.trim_start();
        let new_start = loc.start() as usize + (source_str.len() - trimmed_front.len());
        let trimmed_back = trimmed_front.trim_end();
        let new_end = (loc.end() as usize).saturating_sub(trimmed_front.len() - trimmed_back.len());
        Some(Loc::new(loc.file_hash(), new_start as u32, new_end as u32))
    }

    /// Given a location `Loc` return the source for the location. This include any leading and
    /// trailing whitespace.
    fn source_of_loc_opt(&self, loc: &Loc) -> Option<&str> {
        let file_id = *self.file_mapping().get(&loc.file_hash())?;
        Some(&self.files().source(file_id).ok()?[loc.usize_range()])
    }
}
