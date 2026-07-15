// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Diagnostics describing macro frame info recorded in the source map,
//! emitted only under the "macro-frames" compiler mode (see
//! `shared::macro_frames::MACRO_FRAMES_MODE`) and consumed by the
//! `.macro_frames` compiler tests.

use crate::{diag, diagnostics::DiagnosticReporter, shared::files};
use move_binary_format::file_format as F;
use move_bytecode_source_map::source_map::SourceMap;
use move_ir_types::location::Loc;
use std::collections::BTreeMap;

/// Emit diagnostics about macro frame info for the "macro-frames" test mode.
/// Produces one consolidated diagnostic per function with:
/// - Primary label at the top-level macro call site
/// - Secondary labels at each expansion frame's call_loc
/// - Frame transitions note showing debugger-visible frame stack changes
pub(super) fn emit_macro_frame_diagnostics(
    reporter: &DiagnosticReporter,
    source_map: &SourceMap,
    compiled_module: &F::CompiledModule,
    mapped_files: &files::MappedFiles,
) {
    use move_bytecode_source_map::source_map::{MacroFrameInfoEntry, MacroFrameKind};

    fn format_kind(entry: &MacroFrameInfoEntry) -> String {
        match &entry.kind {
            MacroFrameKind::MacroBody {
                module_addr,
                module_name,
                function_name,
            } => format!(
                "MacroBody({}::{}::{})",
                module_addr.to_hex_literal(),
                module_name,
                function_name,
            ),
            MacroFrameKind::Lambda => "Lambda".to_string(),
            MacroFrameKind::Argument => "Argument".to_string(),
        }
    }

    fn format_kind_short(kind: &MacroFrameKind) -> &'static str {
        match kind {
            MacroFrameKind::MacroBody { .. } => "MacroBody",
            MacroFrameKind::Lambda => "Lambda",
            MacroFrameKind::Argument => "Argument",
        }
    }

    fn format_kind_stack(entry: &MacroFrameInfoEntry) -> String {
        match &entry.kind {
            MacroFrameKind::MacroBody { function_name, .. } => {
                format!("MacroBody({})", function_name)
            }
            MacroFrameKind::Lambda => "Lambda".to_string(),
            MacroFrameKind::Argument => "Argument".to_string(),
        }
    }

    /// Formats a stack with frame indices so same-looking frames remain distinguishable.
    fn format_stack_indexed(frames: &[MacroFrameInfoEntry], stack: &[u32]) -> String {
        if stack.is_empty() {
            return "[]".to_string();
        }
        let entries: Vec<String> = stack
            .iter()
            .map(|&idx| format!("{}:{}", idx, format_kind_stack(&frames[idx as usize])))
            .collect();
        format!("[{}]", entries.join(", "))
    }

    /// Formats a leaf frame index for diagnostics, using `user` for no active frame.
    fn format_frame_idx(frame_idx: Option<u32>) -> String {
        frame_idx
            .map(|idx| idx.to_string())
            .unwrap_or_else(|| "user".to_string())
    }

    /// Formats the frame index plus kind without macro/function names for mismatch messages.
    fn format_frame_idx_kind(frames: &[MacroFrameInfoEntry], frame_idx: u32) -> String {
        let entry = &frames[frame_idx as usize];
        format!("{}:{}", frame_idx, format_kind_short(&entry.kind))
    }

    /// Build the frame stack for a given expansion index. It's intended
    /// to mirror frame transitions that would be visible in the debugger.
    fn build_frame_stack(frames: &[MacroFrameInfoEntry], idx: Option<u32>) -> Vec<u32> {
        let Some(i) = idx else { return vec![] };
        let mut stack = vec![];
        let mut current = Some(i);
        while let Some(ci) = current {
            stack.push(ci);
            current = frames[ci as usize].parent_index;
        }
        stack.reverse();
        stack
    }

    /// Formats a source location as `<line>: `<trimmed source line>`` for
    /// use in frame transition diagnostic notes (e.g., `12: \`quad!(v)\``).
    /// Returns `"?"` if the location cannot be resolved.
    fn resolve_loc_to_line(loc: &Loc, mapped_files: &files::MappedFiles) -> String {
        let Some(pos) = mapped_files.start_position_opt(loc) else {
            return "?".to_string();
        };
        let display_line = pos.user_line();
        // line_to_loc_opt uses codespan's line_range which expects 0-indexed
        let line_content = mapped_files
            .line_to_loc_opt(&loc.file_hash(), pos.line_offset())
            .and_then(|line_loc| mapped_files.source_of_loc_opt(&line_loc))
            .map(|s| s.trim())
            .unwrap_or("?");
        format!("{}: `{}`", display_line, line_content)
    }

    /// Build frame transition descriptions using arrow notation.
    /// Each line shows the instruction's source location and the frame stack
    /// change: `{line}: {source} {prev_stack} -> {new_stack}`.
    /// Uses instruction locations from code_map (not frame call_loc) so that
    /// the output reflects where the debugger would be when the transition happens.
    ///
    /// Also checks frame/location consistency: the frame an instruction
    /// is attributed to describes a certain expansion level, and the
    /// instruction's location must reflect this. In particular, an
    /// instruction attributed to an expansion frame must not have a
    /// location outside that frame's source range.
    fn build_frame_transitions(
        fname: &str,
        frames: &[MacroFrameInfoEntry],
        frame_map: &[(F::CodeOffset, Option<u32>)],
        code_map: &BTreeMap<F::CodeOffset, Loc>,
        mapped_files: &files::MappedFiles,
    ) -> String {
        let mut result = format!("Frame transitions ({}):\n", fname);
        let mut prev_stack: Vec<u32> = vec![];
        let mut prev_frame_idx: Option<u32> = None;
        for &(pc, frame_idx) in frame_map {
            let stack = build_frame_stack(frames, frame_idx);
            let frame_idx_changed = frame_idx != prev_frame_idx;
            let stack_changed = stack != prev_stack;
            let instr_line = code_map
                .range(..=pc)
                .next_back()
                .map(|(_, loc)| resolve_loc_to_line(loc, mapped_files))
                .unwrap_or_else(|| "?".to_string());
            let mut details = vec![];

            // Each instruction's frame_map entry (frame_idx) points to a
            // MacroFrameInfoEntry in the `frames` array. The instruction's
            // source location must fall within that entry's source range.
            if let Some(idx) = frame_idx {
                let frame = &frames[idx as usize];
                if let Some((_, instr_loc)) = code_map.range(..=pc).next_back()
                    && !frame.source_loc.contains(instr_loc)
                {
                    let frame_ref = format_frame_idx_kind(frames, idx);
                    details.push(format!(
                        "!! frame/location mismatch: instruction attributed to {}, \
                             but location is outside {} source range",
                        frame_ref, frame_ref,
                    ));
                }
            }

            if frame_idx_changed && !stack_changed {
                details.push(format!(
                    "!! frame index changed without stack transition: {} -> {}",
                    format_frame_idx(prev_frame_idx),
                    format_frame_idx(frame_idx),
                ));
            }

            if !frame_idx_changed && stack_changed {
                details.push(format!(
                    "!! stack transition without frame index change: index stayed {}",
                    format_frame_idx(frame_idx),
                ));
            }

            if !stack_changed && details.is_empty() {
                prev_frame_idx = frame_idx;
                continue;
            }

            let prev_fmt = format_stack_indexed(frames, &prev_stack);
            let curr_fmt = format_stack_indexed(frames, &stack);
            result.push_str(&format!(
                "  {}\n      {} -> {}\n",
                instr_line, prev_fmt, curr_fmt
            ));
            for detail in details {
                result.push_str(&format!("      {}\n", detail));
            }
            prev_stack = stack;
            prev_frame_idx = frame_idx;
        }
        if result.ends_with('\n') {
            result.pop();
        }
        result
    }

    for (fdef_idx, fsm) in source_map.function_source_maps() {
        if fsm.macro_frame_info.is_empty() {
            continue;
        }
        let frames = &fsm.macro_frame_info;

        let func_idx = fdef_idx as usize;
        let fname = if func_idx < compiled_module.function_defs.len() {
            let fdef = &compiled_module.function_defs[func_idx];
            let fhandle = &compiled_module.function_handles[fdef.function.0 as usize];
            compiled_module.identifiers[fhandle.name.0 as usize]
                .as_str()
                .to_string()
        } else {
            format!("function_{}", fdef_idx)
        };

        // Find the first top-level frame for the primary label
        let primary_idx = frames
            .iter()
            .position(|e| e.parent_index.is_none())
            .unwrap_or(0);
        let primary_loc = frames[primary_idx].call_loc;
        let primary_msg = format!("[{}] {}", primary_idx, format_kind(&frames[primary_idx]));

        // Build secondary labels, merging frames at the same call_loc.
        // Use BTreeMap<Loc, _> so labels are ordered by source position.
        let mut labels_by_loc: BTreeMap<Loc, Vec<(usize, &MacroFrameInfoEntry)>> = BTreeMap::new();
        for (idx, entry) in frames.iter().enumerate() {
            if idx == primary_idx {
                continue;
            }
            labels_by_loc
                .entry(entry.call_loc)
                .or_default()
                .push((idx, entry));
        }

        let secondary_labels: Vec<(Loc, String)> = labels_by_loc
            .into_iter()
            .map(|(loc, entries)| {
                if entries.len() == 1 {
                    // Single entry: use full kind description
                    let (idx, entry) = &entries[0];
                    return (loc, format!("[{}] {}", idx, format_kind(entry)));
                }
                // Multiple entries at same loc: group by kind, merge indices
                let mut by_kind: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
                for (idx, entry) in &entries {
                    by_kind
                        .entry(format_kind_short(&entry.kind))
                        .or_default()
                        .push(*idx);
                }
                let parts: Vec<String> = by_kind
                    .into_iter()
                    .map(|(kind, indices)| {
                        let idx_str = indices
                            .iter()
                            .map(|i| i.to_string())
                            .collect::<Vec<_>>()
                            .join(",");
                        format!("[{}] {}", idx_str, kind)
                    })
                    .collect();
                (loc, parts.join(", "))
            })
            .collect();

        let transitions_note = build_frame_transitions(
            &fname,
            frames,
            &fsm.macro_frame_map,
            &fsm.code_map,
            mapped_files,
        );

        let mut diag = diag!(IDE::MacroFrameInfo, (primary_loc, primary_msg));
        for (loc, msg) in secondary_labels {
            diag.add_secondary_label((loc, msg));
        }
        diag.add_note(transitions_note);
        reporter.add_diag(diag);
    }
}
