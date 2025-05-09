// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module contains logic for storing information about both the use identifier (source file is specified wherever an instance of this
//! struct is used) and the definition identifier

use std::{
    cmp,
    collections::{BTreeMap, BTreeSet},
};

use lsp_types::Position;

use move_command_line_common::files::FileHash;
use move_compiler::shared::files::MappedFiles;
use move_ir_types::location::*;
use move_symbol_pool::Symbol;

#[derive(Debug, Clone, Eq)]
pub struct UseDef {
    /// Column where the (use) identifier location starts on a given line (use this field for
    /// sorting uses on the line)
    pub col_start: u32,
    /// Column where the (use) identifier location ends on a given line
    pub col_end: u32,
    /// Location of the definition
    pub def_loc: Loc,
    /// Location of the type definition
    pub type_def_loc: Option<Loc>,
}

type LineOffset = u32;
/// Maps a line number to a list of use-def-s on a given line (use-def set is sorted by col_start)
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UseDefMap(BTreeMap<LineOffset, BTreeSet<UseDef>>);

/// Location of a use's identifier
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Copy)]
pub struct UseLoc {
    /// File where this use identifier starts
    pub fhash: FileHash,
    /// Location where this use identifier starts
    pub start: Position,
    /// Column (on the same line as start)  where this use identifier ends
    pub col_end: u32,
}

pub type References = BTreeMap<Loc, BTreeSet<UseLoc>>;

//**************************************************************************************************
// Impls
//**************************************************************************************************

impl Ord for UseDef {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.col_start.cmp(&other.col_start)
    }
}

impl PartialOrd for UseDef {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for UseDef {
    fn eq(&self, other: &Self) -> bool {
        self.col_start == other.col_start
    }
}

impl UseDef {
    pub fn new(
        references: &mut References,
        alias_lengths: &BTreeMap<Position, usize>,
        use_fhash: FileHash,
        use_start: Position,
        def_loc: Loc,
        use_name: &Symbol,
        type_def_loc: Option<Loc>,
    ) -> Self {
        // Normally, we compute the length of the identifier as the length
        // of the string that represents it as this string is the same
        // in the source file and in the AST. However, for aliased module
        // accesses, the string in the source represents the alias and
        // the string in the AST represents the actual (non-aliased) module
        // name - we need to retrieve the correct source-level length
        // from the map, otherwise on-hover may not work correctly
        // if AST-level and source-level lengths are different.
        //
        // To illustrate it with an example, in the source we may have:
        //
        // module Symbols::M9 {
        //     use Symbols::M1 as ALIAS_M1;
        //
        //    struct SomeStruct  {
        //        some_field: ALIAS_M1::AnotherStruct,
        //    }
        // }
        //
        // In the (typed) AST we will however have:
        //
        // module Symbols::M9 {
        //     use Symbols::M1 as ALIAS_M1;
        //
        //    struct SomeStruct  {
        //        some_field: M1::AnotherStruct,
        //    }
        // }
        //
        // As a result, when trying to connect the "use" of module alias with
        // the module definition, at the level of (typed) AST we will have
        // identifier of the wrong length which may mess up on-hover and go-to-default
        // (hovering over a portion of a longer alias may not trigger either).

        let use_name_len = match alias_lengths.get(&use_start) {
            Some(l) => *l,
            None => use_name.len(),
        };
        let col_end = use_start.character + use_name_len as u32;
        let use_loc = UseLoc {
            fhash: use_fhash,
            start: use_start,
            col_end,
        };

        references.entry(def_loc).or_default().insert(use_loc);
        Self {
            col_start: use_start.character,
            col_end,
            def_loc,
            type_def_loc,
        }
    }

    /// Given a UseDef, modify just the use name and location (to make it represent an alias).
    pub fn rename_use(
        &mut self,
        references: &mut References,
        new_name: Symbol,
        new_start: Position,
        new_fhash: FileHash,
    ) {
        self.col_start = new_start.character;
        self.col_end = new_start.character + new_name.len() as u32;
        let new_use_loc = UseLoc {
            fhash: new_fhash,
            start: new_start,
            col_end: self.col_end,
        };

        references
            .entry(self.def_loc)
            .or_default()
            .insert(new_use_loc);
    }

    pub fn col_start(&self) -> u32 {
        self.col_start
    }

    pub fn col_end(&self) -> u32 {
        self.col_end
    }

    pub fn def_loc(&self) -> Loc {
        self.def_loc
    }

    // use_line is zero-indexed
    pub fn render(
        &self,
        f: &mut dyn std::io::Write,
        mapped_files: &MappedFiles,
        use_line: u32,
        use_file_content: &str,
        def_file_content: &str,
    ) -> std::io::Result<()> {
        let UseDef {
            col_start,
            col_end,
            def_loc,
            type_def_loc,
        } = self;
        let uident = use_ident(use_file_content, use_line, *col_start, *col_end);
        writeln!(f, "Use: '{uident}', start: {col_start}, end: {col_end}")?;
        let dstart = mapped_files.start_position(def_loc);
        let dline = dstart.line_offset() as u32;
        let dcharacter = dstart.column_offset() as u32;
        let dident = def_ident(def_file_content, dline, dcharacter);
        writeln!(f, "Def: '{dident}', line: {dline}, def char: {dcharacter}")?;
        if let Some(ty_loc) = type_def_loc {
            let tdstart = mapped_files.start_position(ty_loc);
            let tdline = tdstart.line_offset() as u32;
            let tdcharacter = tdstart.column_offset() as u32;
            if let Some((_, type_def_file_content)) = mapped_files.get(&ty_loc.file_hash()) {
                let type_dident = def_ident(&type_def_file_content, tdline, tdcharacter);
                writeln!(
                    f,
                    "TypeDef: '{type_dident}', line: {tdline}, char: {tdcharacter}"
                )
            } else {
                writeln!(f, "TypeDef: INCORRECT INFO")
            }
        } else {
            writeln!(f, "TypeDef: no info")
        }
    }
}

impl Default for UseDefMap {
    fn default() -> Self {
        Self::new()
    }
}

impl UseDefMap {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub fn insert(&mut self, key: u32, val: UseDef) {
        self.0.entry(key).or_default().insert(val);
    }

    pub fn get(&self, key: u32) -> Option<BTreeSet<UseDef>> {
        self.0.get(&key).cloned()
    }

    pub fn elements(self) -> BTreeMap<u32, BTreeSet<UseDef>> {
        self.0
    }

    pub fn count(&self) -> usize {
        self.0.len()
    }

    pub fn extend(&mut self, use_defs: BTreeMap<u32, BTreeSet<UseDef>>) {
        for (k, v) in use_defs {
            self.0.entry(k).or_default().extend(v);
        }
    }
}

fn use_ident(use_file_content: &str, use_line: u32, col_start: u32, col_end: u32) -> String {
    if let Some(line) = use_file_content.lines().nth(use_line as usize) {
        if let Some((start, _)) = line.char_indices().nth(col_start as usize) {
            if let Some((end, _)) = line.char_indices().nth(col_end as usize) {
                return line[start..end].into();
            }
        }
    }
    "INVALID USE IDENT".to_string()
}

fn def_ident(def_file_content: &str, def_line: u32, col_start: u32) -> String {
    if let Some(line) = def_file_content.lines().nth(def_line as usize) {
        if let Some((start, _)) = line.char_indices().nth(col_start as usize) {
            let end = line[start..]
                .char_indices()
                .find(|(_, c)| !c.is_alphanumeric() && *c != '_' && *c != '$')
                .map_or(line.len(), |(i, _)| start + i);
            return line[start..end].into();
        }
    }
    "INVALID DEF IDENT".to_string()
}
