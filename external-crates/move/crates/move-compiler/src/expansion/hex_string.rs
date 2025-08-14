// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{diag, diagnostics::Diagnostic, parser::syntax::make_loc};
use move_ir_types::location::*;

pub enum HexstringError {
    InvalidChar(Loc, char),
    OddLength(Loc),
}

impl HexstringError {
    pub fn into_diagnostic(self) -> Diagnostic {
        match self {
            HexstringError::InvalidChar(loc, c) => diag!(
                Syntax::InvalidHexString,
                (loc, format!("Invalid hexadecimal character: '{c}'"),
            )),
            HexstringError::OddLength(loc) => diag!(
                Syntax::InvalidHexString,
                (
                    loc,
                    "Odd number of characters in hex string. Expected 2 hexadecimal digits for each \
                     byte".to_string(),
                )
            ),
        }
    }
}
pub fn decode(loc: Loc, s: &str) -> Result<Vec<u8>, HexstringError> {
    match hex::decode(s) {
        Ok(vec) => Ok(vec),
        Err(hex::FromHexError::InvalidHexCharacter { c, index }) => {
            let filename = loc.file_hash();
            let start_offset = loc.start() as usize;
            let offset = start_offset + 2 + index;
            let loc = make_loc(filename, offset, offset);
            Err(HexstringError::InvalidChar(loc, c))
        }
        Err(hex::FromHexError::OddLength) => Err(HexstringError::OddLength(loc)),
        Err(_) => unreachable!("unexpected error parsing hex byte string value"),
    }
}
