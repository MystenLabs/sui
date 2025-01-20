// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use lsp_types::Position;
use move_command_line_common::files::FileHash;
use move_compiler::{
    shared::files::MappedFiles, unit_test::filter_test_members::UNIT_TEST_POISON_FUN_NAME,
};
use move_ir_types::location::*;
use move_symbol_pool::Symbol;

//**************************************************************************************************
// Location Conversions
//**************************************************************************************************

/// Converts a location from the byte index format to the line/character (Position) format, where
/// line/character are 0-based.
pub fn offset_to_lsp_position(
    files: &MappedFiles,
    file_hash: &FileHash,
    offset: ByteIndex,
) -> Option<Position> {
    let loc_posn = files.byte_index_to_position_opt(file_hash, offset)?;
    let result = Position {
        line: loc_posn.line_offset() as u32,
        character: loc_posn.column_offset() as u32,
    };
    Some(result)
}

pub fn loc_start_to_lsp_position_opt(files: &MappedFiles, loc: &Loc) -> Option<Position> {
    let start_loc_posn = files.start_position_opt(loc)?;
    let result = Position {
        line: start_loc_posn.line_offset() as u32,
        character: start_loc_posn.column_offset() as u32,
    };
    Some(result)
}

pub fn loc_end_to_lsp_position_opt(files: &MappedFiles, loc: &Loc) -> Option<Position> {
    let end_loc_posn = files.end_position_opt(loc)?;
    let result = Position {
        line: end_loc_posn.line_offset() as u32,
        character: end_loc_posn.column_offset() as u32,
    };
    Some(result)
}

pub fn lsp_position_to_loc(
    files: &MappedFiles,
    file_hash: FileHash,
    pos: &Position,
) -> Option<Loc> {
    let line_offset = pos.line;
    let char_offset = pos.character;
    files.line_char_offset_to_loc_opt(file_hash, line_offset, char_offset)
}

/// Some functions defined in a module need to be ignored.
pub fn ignored_function(name: Symbol) -> bool {
    // In test mode (that's how IDE compiles Move source files),
    // the compiler inserts a dummy function preventing preventing
    // publishing of modules compiled in test mode. We need to
    // ignore its definition to avoid spurious on-hover display
    // of this function's info whe hovering close to `module` keyword.
    name == UNIT_TEST_POISON_FUN_NAME
}
