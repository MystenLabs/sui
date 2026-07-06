// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// -------------------------------------------------------------------------------------------------
// Harness for Structring Testing
// -------------------------------------------------------------------------------------------------

use move_binary_format::normalized::ModuleId;
use move_core_types::account_address::AccountAddress;
use move_symbol_pool::Symbol;

pub fn structuring_unit_test(file_path: &std::path::Path) -> String {
    use crate::structuring::ast::Input as In;

    use petgraph::graph::NodeIndex;
    use std::{
        collections::BTreeMap,
        fs::File,
        io::{BufRead, BufReader},
        path::Path,
    };

    fn parse_input(path: &Path) -> Result<Vec<In>, Vec<String>> {
        let file = match File::open(path) {
            Ok(f) => f,
            Err(e) => return Err(vec![format!("Failed to open file: {}", e)]),
        };

        let reader = BufReader::new(file);
        let mut nodes = Vec::new();
        let mut errors = Vec::new();

        for (line_num, line_result) in reader.lines().enumerate() {
            let line_number = line_num + 1;
            let orig = match line_result {
                Ok(line) => line,
                Err(e) => {
                    errors.push(format!("Error reading line {}: {}", line_number, e));
                    continue;
                }
            };

            let line = orig.split("//").next().unwrap().trim();

            if line.is_empty() {
                continue;
            }

            let parts: Vec<_> = line.split(',').map(str::trim).collect();

            match parts.as_slice() {
                ["cond", a, b, c] => match (a.parse::<u32>(), b.parse::<u32>(), c.parse::<u32>()) {
                    // Match the translate.rs normalization: a `cond` whose two arms target
                    // the same label is a `code` with a dead condition.
                    (Ok(a), Ok(b), Ok(c)) if b == c => {
                        nodes.push(In::Code(a.into(), a.into(), Some(b.into())))
                    }
                    (Ok(a), Ok(b), Ok(c)) => {
                        nodes.push(In::Condition(a.into(), a.into(), b.into(), c.into()))
                    }
                    _ => errors.push(format!("Malformed line {}: {}", line_number, orig)),
                },
                ["code", a, b] => match (a.parse::<u32>(), b.parse::<u32>()) {
                    (Ok(a), Ok(b)) => nodes.push(In::Code(a.into(), a.into(), Some(b.into()))),
                    _ => errors.push(format!("Malformed line {}: {}", line_number, orig)),
                },
                ["code", a] => match a.parse::<u32>() {
                    Ok(a) => nodes.push(In::Code(a.into(), a.into(), None)),
                    _ => errors.push(format!("Malformed line {}: {}", line_number, orig)),
                },
                [head, rest @ ..] if *head == "variants" => {
                    if rest.len() < 2 {
                        errors.push(format!("Malformed line {}: {}", line_number, orig));
                        continue;
                    }

                    let mut iter = rest.iter();
                    let first = match iter.next().unwrap().parse::<u32>() {
                        Ok(n) => n,
                        Err(_) => {
                            errors.push(format!("Malformed line {}: {}", line_number, orig));
                            continue;
                        }
                    };

                    let mut others: Vec<NodeIndex> = Vec::new();
                    let mut ok = true;
                    for r in iter {
                        match r.parse::<u32>() {
                            Ok(n) => others.push(n.into()),
                            Err(_) => {
                                errors.push(format!("Malformed line {}: {}", line_number, orig));
                                ok = false;
                                break;
                            }
                        }
                    }
                    let others = others
                        .into_iter()
                        .map(|other| (Symbol::from(format!("{}", other.index())), other))
                        .collect::<Vec<_>>();
                    let mid: ModuleId<Symbol> = ModuleId {
                        address: AccountAddress::ZERO,
                        name: Symbol::from("M"),
                    };
                    let e = Symbol::from("E");
                    if ok {
                        nodes.push(In::Variants(first.into(), first.into(), (mid, e), others));
                    }
                }
                _ => errors.push(format!("Malformed line {}: {}", line_number, orig)),
            }
        }

        if errors.is_empty() {
            Ok(nodes)
        } else {
            Err(errors)
        }
    }

    let input = match parse_input(file_path) {
        Ok(input) => input
            .into_iter()
            .map(|entry| (entry.label(), entry))
            .collect::<BTreeMap<_, _>>(),
        Err(errs) => return errs.join("\n"),
    };
    if !input.contains_key(&0.into()) {
        return "Expected an entry point `0`, but none was found".to_owned();
    }
    let config = crate::config::Config::default();
    // `run_structuring_test` exercises the structurer in isolation on a tiny `.stt` fixture
    // - there's no `terms` map (term reconstruction is part of `translate.rs`, not the
    // structurer). Pass an empty map; `bodies_equivalent` treats every block with no entry
    // in `terms` as "no body to compare", drops them all via `filter_map`, and the resulting
    // empty s1/s2 lists trivially compare equal - i.e., the guard is bypassed. That's the
    // right behavior for these `.stt` shape tests: they pin the structurer's CFG-to-AST
    // mapping, and the content-level guard would only mask the shape regressions they
    // exist to catch.
    //
    // Some fixtures pin known-pathological CFGs that the current structurer can't handle
    // (e.g. tangled multi-loop residues that need NMG V-B). `catch_unwind` turns the panic
    // into a stable snap so the suite still runs and the failure surfaces as a diff rather
    // than a process-killing crash.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::structuring::structure(&config, input, 0.into())
    }));
    let (structured, report) = match result {
        Ok(pair) => pair,
        Err(panic) => {
            let msg = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "<non-string panic payload>".to_string());
            return format!("// STRUCTURING PANICKED: {msg}\n");
        }
    };
    // Surface structurer residue in the snapshot so a regression shows up as a snap diff
    // rather than silently passing on shape match. Two categories:
    //   - `dropped_blocks`: input `Block(code)` entries that never got emitted - real
    //     source missing from the output.
    //   - `residual_jumps`: surviving `Jump(_, label)` nodes that reach emission - the
    //     body is present but contains explicit unstructured gotos. `.stt` fixtures pin
    //     only the structured form, so this notice replaces the inline `goto` rendering
    //     `to_test_string` produces.
    let body = structured.to_test_string();
    let mut prelude = String::new();
    if !report.dropped_blocks.is_empty() {
        let ids: Vec<String> = report
            .dropped_blocks
            .iter()
            .map(|n| n.to_string())
            .collect();
        prelude.push_str(&format!("// unstructured blocks: {}\n", ids.join(", ")));
    }
    if !report.residual_jumps.is_empty() {
        let ids: Vec<String> = report
            .residual_jumps
            .iter()
            .map(|n| n.to_string())
            .collect();
        prelude.push_str(&format!("// residual jumps to: {}\n", ids.join(", ")));
    }
    format!("{prelude}{body}")
}
