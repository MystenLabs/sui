// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod ast;
mod refinement;
mod structuring;
pub mod translate;

use move_stackless_bytecode_2::stackless::ast as S;
use petgraph::graph::NodeIndex;

use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

use crate::translate::module;

// -------------------------------------------------------------------------------------------------
// Main Entry Points
// -------------------------------------------------------------------------------------------------

pub fn decompile_module(module_: S::Module) -> crate::ast::Module {
    module(module_)
}

// -------------------------------------------------------------------------------------------------
// Entry Points for Testing
// -------------------------------------------------------------------------------------------------

pub fn structuring_unit_test(file_path: &Path) -> String {
    use structuring::ast::Input as In;

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
                    (Ok(a), Ok(b), Ok(c)) => nodes.push(In::Condition(
                        a.into(),
                        (a.into(), false),
                        b.into(),
                        c.into(),
                    )),
                    _ => errors.push(format!("Malformed line {}: {}", line_number, orig)),
                },
                ["code", a, b] => match (a.parse::<u32>(), b.parse::<u32>()) {
                    (Ok(a), Ok(b)) => {
                        nodes.push(In::Code(a.into(), (a.into(), false), Some(b.into())))
                    }
                    _ => errors.push(format!("Malformed line {}: {}", line_number, orig)),
                },
                ["code", a] => match a.parse::<u32>() {
                    Ok(a) => nodes.push(In::Code(a.into(), (a.into(), false), None)),
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

                    if ok {
                        nodes.push(In::Variants(first.into(), (first.into(), false), others));
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
    let structured = structuring::structure(input, 0.into());
    structured.to_test_string()
}
