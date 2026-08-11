// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tab completion for the db-shell.
//!
//! Completes:
//!   - Command names
//!   - Path arguments: resolves the parent directory and lists its children
//!
//! Sequence numbers inside /checkpoints/seq/ are NOT completed because the
//! integer space is too large. Digest prefixes are completed up to the 30-entry limit.

use rustyline::Context as RlContext;
use rustyline::Helper;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use std::sync::Arc;

use crate::db_shell::{
    backend::Backend,
    vfs::{VfsPath, resolve_path},
};

const COMMANDS: &[&str] = &[
    "ls", "cd", "cat", "dbg", "bcs", "rm", "pwd", "help", "exit", "quit",
];
const DEFAULT_COMPLETION_LIMIT: usize = 30;

pub struct ShellHelper {
    pub backend: Arc<dyn Backend>,
    pub cwd: VfsPath,
}

impl Helper for ShellHelper {}
impl Validator for ShellHelper {}
impl Highlighter for ShellHelper {}
impl Hinter for ShellHelper {
    type Hint = String;
}

impl Completer for ShellHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &RlContext,
    ) -> Result<(usize, Vec<Pair>), ReadlineError> {
        let line_so_far = &line[..pos];
        let tokens: Vec<&str> = line_so_far.split_whitespace().collect();

        // Determine what we're completing.
        let completing_command =
            tokens.is_empty() || (tokens.len() == 1 && !line_so_far.ends_with(' '));

        if completing_command {
            let prefix = tokens.first().copied().unwrap_or("");
            let candidates: Vec<Pair> = COMMANDS
                .iter()
                .filter(|cmd| cmd.starts_with(prefix))
                .map(|cmd| Pair {
                    display: cmd.to_string(),
                    replacement: format!("{cmd} "),
                })
                .collect();
            let start = line_so_far.len() - prefix.len();
            return Ok((start, candidates));
        }

        // Completing a path argument — find the token being typed.
        let (path_prefix, token_start) = if line_so_far.ends_with(' ') {
            ("", pos)
        } else {
            let last = tokens.last().copied().unwrap_or("");
            // Don't try to complete flags like --limit.
            if last.starts_with('-') {
                return Ok((pos, vec![]));
            }
            let start = pos - last.len();
            (last, start)
        };

        let candidates = complete_path(&self.backend, &self.cwd, path_prefix);
        Ok((token_start, candidates))
    }
}

fn complete_path(backend: &Arc<dyn Backend>, cwd: &VfsPath, prefix: &str) -> Vec<Pair> {
    // Split prefix into (parent_path_str, name_prefix).
    let (parent_str, name_prefix) = if let Some(slash) = prefix.rfind('/') {
        (&prefix[..=slash], &prefix[slash + 1..])
    } else {
        ("", prefix)
    };

    // Resolve the parent to a VfsPath.
    let parent_path = if parent_str.is_empty() {
        cwd.clone()
    } else {
        match resolve_path(cwd, parent_str) {
            Ok(p) => p,
            Err(_) => return vec![],
        }
    };

    // Do NOT complete sequence number children of /checkpoints/seq — too many.
    if matches!(parent_path, VfsPath::CheckpointsSeqRoot) {
        return vec![];
    }

    let entries = match backend.ls_children(&parent_path, DEFAULT_COMPLETION_LIMIT) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    entries
        .into_iter()
        .filter(|e| e.name.starts_with(name_prefix))
        .map(|e| {
            let suffix = if e.is_dir { "/" } else { "" };
            let replacement = format!("{parent_str}{}{suffix}", e.name);
            Pair {
                display: format!("{}{suffix}", e.name),
                replacement,
            }
        })
        .collect()
}

/// Parse a path string, returning `None` if it resolves to a sequence-number
/// level that should not be completed (to avoid the huge integer space).
pub fn is_seq_cursor_level(path: &str) -> bool {
    // Don't complete paths that would iterate /checkpoints/seq/ without a prefix
    path.trim_end_matches('/') == "/checkpoints/seq"
}
