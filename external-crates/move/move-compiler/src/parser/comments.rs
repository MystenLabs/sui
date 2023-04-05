// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{diag, diagnostics::Diagnostics};
use move_command_line_common::{character_sets::is_permitted_chars, files::FileHash};
use move_ir_types::location::*;
use std::collections::BTreeMap;

/// Types to represent comments.
pub type CommentMap = BTreeMap<FileHash, MatchedFileCommentMap>;
pub type MatchedFileCommentMap = BTreeMap<u32, String>;
pub type FileCommentMap = BTreeMap<(u32, u32), String>;

// We restrict strings to only ascii visual characters (0x20 <= c <= 0x7E) or a permitted newline
// character--\r--,--\n--or a tab--\t.
pub fn verify_string(file_hash: FileHash, string: &str) -> Result<(), Diagnostics> {
    match string
        .chars()
        .enumerate()
        .find(|(idx, _)| !is_permitted_chars(string.as_bytes(), *idx))
    {
        None => Ok(()),
        Some((idx, chr)) => {
            let loc = Loc::new(file_hash, idx as u32, idx as u32);
            let msg = format!(
                "Invalid character '{}' found when reading file. Only ASCII printable characters, \
                 tabs (\\t), lf (\\n) and crlf (\\r\\n) are permitted.",
                chr
            );
            Err(Diagnostics::from(vec![diag!(
                Syntax::InvalidCharacter,
                (loc, msg)
            )]))
        }
    }
}
