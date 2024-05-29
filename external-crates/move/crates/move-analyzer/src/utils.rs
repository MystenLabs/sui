// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use codespan_reporting::files::{Files, SimpleFiles};
use lsp_types::Position;
use move_command_line_common::files::FileHash;
use move_compiler::unit_test::filter_test_members::UNIT_TEST_POISON_FUN_NAME;
use move_ir_types::location::*;
use move_symbol_pool::Symbol;
use std::collections::HashMap;

/// Converts a location from the byte index format to the line/character (Position) format, where
/// line/character are 0-based.
pub fn get_loc(
    fhash: &FileHash,
    pos: ByteIndex,
    files: &SimpleFiles<Symbol, String>,
    file_id_mapping: &HashMap<FileHash, usize>,
) -> Option<Position> {
    let id = match file_id_mapping.get(fhash) {
        Some(v) => v,
        None => return None,
    };
    match files.location(*id, pos as usize) {
        Ok(v) => Some(Position {
            // we need 0-based column location
            line: v.line_number as u32 - 1,
            character: v.column_number as u32 - 1,
        }),
        Err(_) => None,
    }
}

/// Convert a move_compiler Position into an lsp_types position
pub fn to_lsp_position(pos: move_compiler::diagnostics::Position) -> Position {
    Position {
        // we need 0-based column location
        line: pos.line as u32 - 1,
        character: pos.column as u32 - 1,
    }
}

pub fn get_start_position_opt(
    pos: &Loc,
    files: &SimpleFiles<Symbol, String>,
    file_id_mapping: &HashMap<FileHash, usize>,
) -> Option<Position> {
    get_loc(&pos.file_hash(), pos.start(), files, file_id_mapping)
}

/// Some functions defined in a module need to be ignored.
pub fn ignored_function(name: Symbol) -> bool {
    // In test mode (that's how IDE compiles Move source files),
    // the compiler inserts an dummy function preventing preventing
    // publishing of modules compiled in test mode. We need to
    // ignore its definition to avoid spurious on-hover display
    // of this function's info whe hovering close to `module` keyword.
    name == UNIT_TEST_POISON_FUN_NAME
}
