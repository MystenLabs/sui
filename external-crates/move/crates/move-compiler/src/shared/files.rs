// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use codespan_reporting::files::{Files, SimpleFiles};
use move_command_line_common::files::FileHash;
use move_ir_types::location::*;
use move_symbol_pool::Symbol;
use std::{
    collections::{hash_map, BTreeMap, HashMap},
    path::PathBuf,
    sync::Arc,
};

//**************************************************************************************************
// Types
//**************************************************************************************************

pub type FileId = usize;
pub type FileName = Symbol;

pub type FilesSourceText = HashMap<FileHash, (FileName, Arc<str>)>;

/// A mapping from file ids to file contents along with the mapping of filehash to fileID.
#[derive(Debug, Clone)]
pub struct MappedFiles {
    files: SimpleFiles<Symbol, Arc<str>>,
    file_mapping: HashMap<FileHash, FileId>,
    file_name_mapping: BTreeMap<FileHash, PathBuf>,
}

/// A file, the line:column start, and line:column end that corresponds to a `Loc`
#[allow(dead_code)]
pub struct FilePositionSpan {
    pub file_hash: FileHash,
    pub start: Position,
    pub end: Position,
}

/// A file and a line:column location in it
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Copy)]
pub struct FilePosition {
    /// File hash
    pub file_hash: FileHash,
    /// Location
    pub position: Position,
}

/// A position holds the byte offset along with the line and column location in a file.
/// Both are zero-indexed.
#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd, Copy)]
pub struct Position {
    // zero-indexed line offset
    line_offset: usize,
    // zero-indexed column offset
    column_offset: usize,
    // zero-indexed byte offset
    byte_offset: usize,
}

/// A file, and the usize start and usize end that corresponds to a `Loc`
pub struct FileByteSpan {
    pub file_hash: FileHash,
    pub byte_span: ByteSpan,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ByteSpan {
    pub start: usize,
    pub end: usize,
}

//**************************************************************************************************
// Mapped Files
//**************************************************************************************************

impl MappedFiles {
    pub fn new(files: FilesSourceText) -> Self {
        let mut simple_files = SimpleFiles::new();
        let mut file_mapping = HashMap::new();
        let mut file_name_mapping = BTreeMap::new();
        for (fhash, (fname, source)) in files {
            let id = simple_files.add(fname, source);
            file_mapping.insert(fhash, id);
            file_name_mapping.insert(fhash, PathBuf::from(fname.as_str()));
        }
        Self {
            files: simple_files,
            file_mapping,
            file_name_mapping,
        }
    }

    pub fn empty() -> Self {
        Self {
            files: SimpleFiles::new(),
            file_mapping: HashMap::new(),
            file_name_mapping: BTreeMap::new(),
        }
    }

    fn extend_(&mut self, other: Self, allow_duplicates: bool) {
        for (file_hash, file_id) in other.file_mapping {
            let Ok(file) = other.files.get(file_id) else {
                debug_assert!(false, "Found a file without a file entry");
                continue;
            };
            let Some(path) = other.file_name_mapping.get(&file_hash) else {
                debug_assert!(false, "Found a file without a path entry");
                continue;
            };
            debug_assert!(
                allow_duplicates || !self.file_mapping.contains_key(&file_hash),
                "Found a repeat file hash"
            );
            let fname = format!("{}", path.to_string_lossy());
            self.add(file_hash, fname.into(), file.source().clone());
        }
    }

    pub fn extend(&mut self, other: Self) {
        self.extend_(other, false)
    }

    pub fn extend_with_duplicates(&mut self, other: Self) {
        self.extend_(other, true)
    }

    pub fn add(&mut self, fhash: FileHash, fname: FileName, source: Arc<str>) {
        let id = self.files.add(fname, source);
        self.file_mapping.insert(fhash, id);
        self.file_name_mapping
            .insert(fhash, PathBuf::from(fname.as_str()));
    }

    pub fn get(&self, fhash: &FileHash) -> Option<(Symbol, Arc<str>)> {
        let file_id = self.file_mapping.get(fhash)?;
        self.files
            .get(*file_id)
            .ok()
            .map(|file| (*file.name(), file.source().clone()))
    }

    /// Find a file hash for a path buffer. Note this is inefficient.
    pub fn file_hash(&self, path: &PathBuf) -> Option<FileHash> {
        for (file_hash, file_path) in &self.file_name_mapping {
            if file_path == path {
                return Some(*file_hash);
            }
        }
        None
    }

    /// Returns the FileHashes for iteration
    pub fn keys(&self) -> hash_map::Keys<'_, FileHash, FileId> {
        self.file_mapping.keys()
    }

    pub fn files(&self) -> &SimpleFiles<Symbol, Arc<str>> {
        &self.files
    }

    pub fn file_mapping(&self) -> &HashMap<FileHash, FileId> {
        &self.file_mapping
    }

    pub fn file_name_mapping(&self) -> &BTreeMap<FileHash, PathBuf> {
        &self.file_name_mapping
    }

    pub fn filename(&self, fhash: &FileHash) -> Symbol {
        let file_id = self.file_mapping().get(fhash).unwrap();
        *self.files().get(*file_id).unwrap().name()
    }

    pub fn file_path(&self, fhash: &FileHash) -> &PathBuf {
        self.file_name_mapping.get(fhash).unwrap()
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
    pub fn position(&self, loc: &Loc) -> FilePositionSpan {
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

    pub fn file_start_position_opt(&self, loc: &Loc) -> Option<FilePosition> {
        self.position_opt(loc)
            .map(|posn| FilePosition::new(posn.file_hash, posn.start))
    }

    pub fn file_end_position_opt(&self, loc: &Loc) -> Option<FilePosition> {
        self.position_opt(loc)
            .map(|posn| FilePosition::new(posn.file_hash, posn.end))
    }

    pub fn file_size(&self, fhash: &FileHash) -> usize {
        let file_id = *self.file_mapping().get(fhash).unwrap();
        let source = self.files().source(file_id).unwrap();
        source.len()
    }

    pub fn position_opt(&self, loc: &Loc) -> Option<FilePositionSpan> {
        let file_hash = loc.file_hash();
        let start_loc = loc.start() as usize;
        let end_loc = loc.end() as usize;
        let file_id = *self.file_mapping().get(&loc.file_hash())?;
        let start_file_loc = self.files().location(file_id, start_loc).ok()?;
        let end_file_loc = self.files().location(file_id, end_loc).ok()?;
        let posn = FilePositionSpan {
            file_hash,
            start: Position {
                line_offset: start_file_loc.line_number - 1,
                column_offset: start_file_loc.column_number - 1,
                byte_offset: start_loc,
            },
            end: Position {
                line_offset: end_file_loc.line_number - 1,
                column_offset: end_file_loc.column_number - 1,
                byte_offset: end_loc,
            },
        };
        Some(posn)
    }

    pub fn byte_span_opt(&self, loc: &Loc) -> Option<FileByteSpan> {
        let file_hash = loc.file_hash();
        let start = loc.start() as usize;
        let end = loc.end() as usize;
        let posn = FileByteSpan {
            byte_span: ByteSpan { start, end },
            file_hash,
        };
        Some(posn)
    }

    pub fn lsp_range_opt(&self, loc: &Loc) -> Option<lsp_types::Range> {
        let position = self.position_opt(loc)?;
        Some(lsp_types::Range {
            start: position.start.into(),
            end: position.end.into(),
        })
    }

    /// Given a line number and character number (both 0-indexed) in the file return the `Loc` for
    /// the line. Note that the end byte is exclusive in the resultant `Loc`.
    pub fn line_char_offset_to_loc_opt(
        &self,
        file_hash: FileHash,
        line_offset: u32,
        char_offset: u32,
    ) -> Option<Loc> {
        let file_id = self.file_mapping().get(&file_hash)?;
        let line_range = self
            .files()
            .line_range(*file_id, line_offset as usize)
            .ok()?;
        let offset = line_range.start as u32 + char_offset;
        Some(Loc::new(file_hash, offset, offset + 1))
    }

    /// Given a line number (1-indexed) in the file return the `Loc` for the line.
    pub fn line_to_loc_opt(&self, file_hash: &FileHash, line_number: usize) -> Option<Loc> {
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
    pub fn trimmed_loc_opt(&self, loc: &Loc) -> Option<Loc> {
        let source_str = self.source_of_loc_opt(loc)?;
        let trimmed_front = source_str.trim_start();
        let new_start = loc.start() as usize + (source_str.len() - trimmed_front.len());
        let trimmed_back = trimmed_front.trim_end();
        let new_end = (loc.end() as usize).saturating_sub(trimmed_front.len() - trimmed_back.len());
        Some(Loc::new(loc.file_hash(), new_start as u32, new_end as u32))
    }

    /// Given a location `Loc` return the source for the location. This include any leading and
    /// trailing whitespace.
    pub fn source_of_loc_opt(&self, loc: &Loc) -> Option<&str> {
        let file_id = *self.file_mapping().get(&loc.file_hash())?;
        Some(&self.files().source(file_id).ok()?[loc.usize_range()])
    }

    /// Given a file_hash `file` and a byte index `byte_index`, compute its `Position`.
    pub fn byte_index_to_position_opt(
        &self,
        file: &FileHash,
        byte_index: ByteIndex,
    ) -> Option<Position> {
        let file_id = self.file_hash_to_file_id(file)?;
        let byte_position = self.files().location(file_id, byte_index as usize).ok()?;
        let result = Position {
            line_offset: byte_position.line_number - 1,
            column_offset: byte_position.column_number - 1,
            byte_offset: byte_index as usize,
        };
        Some(result)
    }
}

impl From<FilesSourceText> for MappedFiles {
    fn from(value: FilesSourceText) -> Self {
        MappedFiles::new(value)
    }
}

/// Iterator for MappedFiles
pub struct MappedFilesIter<'a> {
    mapped_files: &'a MappedFiles,
    keys_iter: hash_map::Iter<'a, FileHash, FileId>,
}

impl<'a> MappedFilesIter<'a> {
    fn new(mapped_files: &'a MappedFiles) -> Self {
        MappedFilesIter {
            mapped_files,
            keys_iter: mapped_files.file_mapping.iter(),
        }
    }
}

impl<'a> Iterator for MappedFilesIter<'a> {
    type Item = (&'a FileHash, (&'a Symbol, &'a Arc<str>));

    fn next(&mut self) -> Option<Self::Item> {
        let (file_hash, file_id) = self.keys_iter.next()?;
        let Some(file) = self.mapped_files.files.get(*file_id).ok() else {
            eprintln!("Files went wrong.");
            return None;
        };
        Some((file_hash, (file.name(), file.source())))
    }
}

impl<'a> IntoIterator for &'a MappedFiles {
    type Item = (&'a FileHash, (&'a Symbol, &'a Arc<str>));
    type IntoIter = MappedFilesIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        MappedFilesIter::new(self)
    }
}

//**************************************************************************************************
// Impls
//**************************************************************************************************

impl FilePosition {
    pub fn new(file_hash: FileHash, position: Position) -> Self {
        FilePosition {
            file_hash,
            position,
        }
    }

    pub fn file_hash(&self) -> FileHash {
        self.file_hash
    }

    pub fn position(&self) -> Position {
        self.position
    }
}

impl FilePositionSpan {
    /// Return the start position from the span
    pub fn file_start_position(self) -> FilePosition {
        FilePosition::new(self.file_hash, self.start)
    }

    /// Return the end position from the span
    pub fn file_end_position(self) -> FilePosition {
        FilePosition::new(self.file_hash, self.start)
    }
}

impl Position {
    pub fn empty() -> Self {
        Position {
            line_offset: 0,
            column_offset: 0,
            byte_offset: 0,
        }
    }

    /// User-facing (1-indexed) line
    pub fn user_line(&self) -> usize {
        self.line_offset + 1
    }

    /// User-facing (1-indexed) column
    pub fn user_column(&self) -> usize {
        self.column_offset + 1
    }

    /// Line offset (0-indexed)
    pub fn line_offset(&self) -> usize {
        self.line_offset
    }

    /// Column offset (0-indexed)
    pub fn column_offset(&self) -> usize {
        self.column_offset
    }

    /// Btye offset for position (0-indexed)
    pub fn byte_offset(&self) -> usize {
        self.byte_offset
    }
}

//**************************************************************************************************
// LSP Conversions
//**************************************************************************************************

#[allow(clippy::from_over_into)]
impl Into<lsp_types::Position> for FilePosition {
    fn into(self) -> lsp_types::Position {
        lsp_types::Position::new(
            self.position.line_offset() as u32,
            self.position.column_offset() as u32,
        )
    }
}

#[allow(clippy::from_over_into)]
impl Into<lsp_types::Position> for Position {
    fn into(self) -> lsp_types::Position {
        lsp_types::Position::new(self.line_offset() as u32, self.column_offset() as u32)
    }
}
