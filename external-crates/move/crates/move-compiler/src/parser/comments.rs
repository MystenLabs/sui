// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{diag, diagnostics::Diagnostics};
use move_command_line_common::{
    character_sets::{is_permitted_chars, DisplayChar},
    files::FileHash,
};
use move_ir_types::location::*;

// We restrict strings to only ascii visual characters (0x20 <= c <= 0x7E) or a permitted newline
// character--\r--,--\n--or a tab--\t.
pub fn verify_string(file_hash: FileHash, string: &str) -> Result<(), Diagnostics> {
    let chars: Vec<char> = string.chars().collect();
    match chars
        .iter()
        .enumerate()
        .find(|(idx, _)| !is_permitted_chars(&chars, *idx))
    {
        None => Ok(()),
        Some((idx, c)) => {
            let loc = Loc::new(file_hash, idx as u32, idx as u32);
            let msg = format!(
                "Invalid character '{}' found when reading file. \
                For ASCII, only printable characters (tabs '\\t', lf '\\n' and crlf '\\r'+'\\n') \
                are permitted. Unicode can be used in comments and string literals, \
                excluding certain control characters.",
                DisplayChar(*c),
            );
            Err(Diagnostics::from(vec![diag!(
                Syntax::InvalidCharacter,
                (loc, msg)
            )]))
        }
    }
}
