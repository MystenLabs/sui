// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use lsp_types::Position;
use move_command_line_common::files::FileHash;
use move_compiler::{
    shared::files::{self, MappedFiles},
    unit_test::filter_test_members::UNIT_TEST_POISON_FUN_NAME,
};
use move_ir_types::location::*;
use move_symbol_pool::Symbol;

/// Converts a location from the byte index format to the line/character (Position) format, where
/// line/character are 0-based.
pub fn get_loc(fhash: &FileHash, pos: ByteIndex, files: &MappedFiles) -> Option<Position> {
    let loc_posn = files.byte_index_to_position_opt(fhash, pos)?;
    let result = Position {
        line: loc_posn.line_offset() as u32,
        character: loc_posn.column_offset() as u32,
    };
    Some(result)
}

/// Converts a position (line/column) to byte index in the file.
pub fn get_byte_idx(
    pos: Position,
    fhash: FileHash,
    files: &SimpleFiles<Symbol, String>,
    file_id_mapping: &HashMap<FileHash, usize>,
) -> Option<ByteIndex> {
    let Some(file_id) = file_id_mapping.get(&fhash) else {
        return None;
    };
    let Ok(line_range) = files.line_range(*file_id, pos.line as usize) else {
        return None;
    };
    Some(line_range.start as u32 + pos.character)
}

/// Convert a move_compiler Position into an lsp_types position
pub fn to_lsp_position(pos: files::Position) -> Position {
    Position {
        line: pos.line_offset() as u32,
        character: pos.column_offset() as u32,
    }
}

pub fn get_start_position_opt(pos: &Loc, files: &MappedFiles) -> Option<Position> {
    let start_loc_posn = files.start_position_opt(pos)?;
    let result = Position {
        line: start_loc_posn.line_offset() as u32,
        character: start_loc_posn.column_offset() as u32,
    };
    Some(result)
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
