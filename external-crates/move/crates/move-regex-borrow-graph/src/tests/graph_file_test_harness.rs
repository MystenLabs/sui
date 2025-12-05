// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{collections::Graph, references::Ref};

use std::collections::{BTreeMap, BTreeSet};

// -------------------------------------------------------------------------------------------------
// Borrow Arrangements
// -------------------------------------------------------------------------------------------------

// This section defines a simple model of borrow arrangements to generate test cases. These allow
// us to model various borrow states, toward verifying that our graph construction and
// classification logic behaves as expected.

/// A simple semantic model of the borrow state before we build a graph.
#[derive(Clone, Debug)]
enum BorrowOp {
    Local {
        local_id: u8,
        is_mutable: bool,
    },
    Alias {
        base: usize,
        is_mutable: bool,
    },
    BorrowField {
        base: usize,
        label: char,
        is_mutable: bool,
    },
    Call {
        args: Vec<usize>,
        is_mutable: bool, // result
    },
}

#[derive(Clone, Debug)]
enum BorrowTestOp {
    Assign { lhs: usize, rhs: BorrowOp },
    Check { line: usize, writables: Vec<usize> },
}

#[derive(Clone)]
struct BorrowArrangement {
    locals: BTreeSet<u8>,
    ops: Vec<BorrowTestOp>,
}

struct BorrowArrangementParser {
    ops: Vec<BorrowTestOp>,
    mutables: Vec<usize>,
    defined: Vec<usize>,
}

impl BorrowOp {
    #[allow(dead_code)]
    fn is_mutable(&self) -> bool {
        match self {
            BorrowOp::Local { is_mutable, .. } => *is_mutable,
            BorrowOp::Alias { is_mutable, .. } => *is_mutable,
            BorrowOp::BorrowField { is_mutable, .. } => *is_mutable,
            BorrowOp::Call { is_mutable, .. } => *is_mutable,
        }
    }
}

/// Like assert, but returns Err with message instead of panicking.
/// Take multiple message arguments for convenience.
macro_rules! test_assert {
    ($cond:expr, $($msg:expr),+) => {
        if !($cond) {
            let msg = format!($($msg),+);
            return Err(msg);
        }
    };
}

impl BorrowArrangement {
    fn from_file(file: &std::path::Path) -> Result<Self, String> {
        BorrowArrangementParser::parse_from_file(file)
    }
}

impl BorrowArrangementParser {
    fn parse_from_file(file: &std::path::Path) -> Result<BorrowArrangement, String> {
        let mut parser = BorrowArrangementParser {
            ops: Vec::new(),
            mutables: Vec::new(),
            defined: Vec::new(),
        };
        parser.parse(file)?;
        parser.into_borrow_arrangement()
    }

    fn parse(&mut self, file: &std::path::Path) -> Result<(), String> {
        let content = std::fs::read_to_string(file).expect("Unable to read file");
        let lines = content.lines();

        let mut error_msgs = Vec::new();

        // Subsequent lines: operations
        for (ndx, line) in lines.enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("//") {
                continue;
            }

            // Statefully parse the line

            if let Err(msg) = self.parse_line(ndx + 1, line) {
                error_msgs.push(msg);
            }
        }

        if !error_msgs.is_empty() {
            return Err(error_msgs.join("\n"));
        }

        test_assert!(!self.ops.is_empty(), "No operations found in file");
        Ok(())
    }

    fn check_defined(&self, line: usize, index: usize) -> Result<(), String> {
        if !self.defined.contains(&index) {
            return Err(format!(
                "Line {line}: Reference {index} used before definition"
            ));
        }
        Ok(())
    }

    fn check_not_defined(&self, line: usize, index: usize) -> Result<(), String> {
        if self.defined.contains(&index) {
            return Err(format!("Line {line}: Reference {index} already defined"));
        }
        Ok(())
    }

    fn check_mutable(&self, line: usize, index: usize) -> Result<(), String> {
        if !self.mutables.contains(&index) {
            return Err(format!(
                "Line {line}: Reference {index} used as mutable but is not mutable"
            ));
        }
        Ok(())
    }

    fn record_mutability(&mut self, index: usize, is_mutable: bool) {
        if is_mutable {
            self.mutables.push(index);
        }
    }

    fn is_mutable(&self, index: usize) -> bool {
        self.mutables.contains(&index)
    }

    // line  := "writable: n,.."
    //        | <ndx> "=" <ref> "local_"<local>
    //        | <ndx> "=" "alias" <base>
    //        | <ndx> "=" "freeze" <base>
    //        | <ndx> "=" <ref> <base>.<label>
    //        | <ndx> "=" <ref> call(<base>, ...)"
    // base  := <ndx>
    // local := <ndx>
    // ref   := "&" | "&mut "
    // labal := { c | c is a single char }
    fn parse_line(&mut self, line: usize, contents: &str) -> Result<(), String> {
        fn parse_mut_or_imm_opt(contents: &str) -> Option<(bool, &str)> {
            if let Some(rest) = contents.strip_prefix("&mut ") {
                Some((true, rest))
            } else if let Some(rest) = contents.strip_prefix("&") {
                Some((false, rest))
            } else {
                None
            }
        }

        fn parse_usize(line: usize, s: &str) -> Result<usize, String> {
            s.trim()
                .parse()
                .map_err(|_| format!("Line {line}: Expected a number, but got '{}'", s))
        }

        if let Some(writables) = contents.strip_prefix("writable:") {
            let writables = writables.trim();
            // if no writables, then empty vec
            let writables = {
                if writables.is_empty() {
                    vec![]
                } else {
                    writables
                        .split(',')
                        .map(|s| parse_usize(line, s.trim()))
                        .collect::<Result<_, _>>()?
                }
            };
            for writable in &writables {
                self.check_defined(line, *writable)?;
            }
            self.ops.push(BorrowTestOp::Check { line, writables });
            return Ok(());
        }

        let parts: Vec<&str> = contents.split('=').map(|s| s.trim()).collect();
        if parts.len() != 2 {
            return Err(format!("Line {line}: Invalid line format"));
        }
        let lhs = parts[0];
        let rhs = parts[1];

        let lhs = parse_usize(line, lhs)?;
        self.check_not_defined(line, lhs)?;
        // Push the LHS so that even if the RHS is malformed so that we don't produce undefined ref
        // errors later, and we also catch re-definition errors.
        self.defined.push(lhs);

        let mut_opt = parse_mut_or_imm_opt(rhs);

        let (mut_ref, rhs) = match mut_opt {
            Some((is_mutable, rest)) => (Some(is_mutable), rest),
            None => (None, rhs),
        };

        if let Some(rest) = rhs.strip_prefix("alias") {
            test_assert!(mut_ref.is_none(), "Alias cannot specify mutability");

            let base_str = rest.trim();
            let base = parse_usize(line, base_str)?;

            self.check_defined(line, base)?;
            let is_mutable = self.is_mutable(base);

            self.record_mutability(lhs, is_mutable);
            let op = BorrowOp::Alias { base, is_mutable };
            self.ops.push(BorrowTestOp::Assign { lhs, rhs: op });
            Ok(())
        } else if let Some(rest) = rhs.strip_prefix("freeze") {
            test_assert!(mut_ref.is_none(), "Freeze cannot specify mutability");

            let base_str = rest.trim();
            let base = parse_usize(line, base_str)?;

            self.check_defined(line, base)?;
            self.check_mutable(line, base)?;
            let is_mutable = false;

            self.record_mutability(lhs, is_mutable);
            let op = BorrowOp::Alias { base, is_mutable };
            self.ops.push(BorrowTestOp::Assign { lhs, rhs: op });
            Ok(())
        } else if let Some(local_id) = rhs.strip_prefix("local_") {
            test_assert!(mut_ref.is_some(), "Local must specify mutability");
            let is_mutable = mut_ref.unwrap();

            let local_id: u8 = local_id
                .parse()
                .map_err(|_| format!("Line {line}: Invalid local id '{}'", local_id))?;

            self.record_mutability(lhs, is_mutable);
            let op = BorrowOp::Local {
                local_id,
                is_mutable,
            };
            self.ops.push(BorrowTestOp::Assign { lhs, rhs: op });
            Ok(())
        } else if let Some(rest) = rhs.strip_prefix("call(") {
            test_assert!(mut_ref.is_some(), "Call must specify mutability");
            let is_mutable = mut_ref.unwrap();

            let Some(args_str) = rest.strip_suffix(')') else {
                return Err(format!("Line {line}: Did not find suffix ')' for call"));
            };
            let args: Vec<usize> = if args_str.trim().is_empty() {
                vec![]
            } else {
                args_str
                    .split(',')
                    .map(|s| parse_usize(line, s.trim()))
                    .collect::<Result<_, _>>()?
            };

            for arg in &args {
                self.check_defined(line, *arg)?;
            }
            self.record_mutability(lhs, mut_ref.unwrap());
            let op = BorrowOp::Call { args, is_mutable };
            self.ops.push(BorrowTestOp::Assign { lhs, rhs: op });
            Ok(())
        } else if let [base, label] = &rhs.split('.').collect::<Vec<_>>()[..] {
            test_assert!(label.chars().count() == 1, "Invalid field borrow label");
            // Field borrow
            test_assert!(mut_ref.is_some(), "Field borrow must specify mutability");
            let base: usize = parse_usize(line, base)?;
            let Some(label) = label.chars().last() else {
                return Err(format!("Line {line}: Invalid field borrow, missing label"));
            };

            self.check_defined(line, base)?;
            let is_mutable = mut_ref.unwrap();
            if is_mutable {
                self.check_mutable(line, base)?;
            }

            self.record_mutability(lhs, is_mutable);
            self.ops.push(BorrowTestOp::Assign {
                lhs,
                rhs: BorrowOp::BorrowField {
                    base,
                    label,
                    is_mutable,
                },
            });
            Ok(())
        } else {
            Err(format!("Line {line}: Invalid RHS format"))
        }
    }

    fn into_borrow_arrangement(self) -> Result<BorrowArrangement, String> {
        let BorrowArrangementParser { ops, .. } = self;
        let locals = ops
            .iter()
            .filter_map(|op| {
                if let BorrowTestOp::Assign {
                    rhs: BorrowOp::Local { local_id, .. },
                    ..
                } = op
                {
                    Some(*local_id)
                } else {
                    None
                }
            })
            .collect();

        let arr = BorrowArrangement { locals, ops };
        Ok(arr)
    }
}

// -------------------------------------------------------------------------------------------------
// Graph Testing
// -------------------------------------------------------------------------------------------------
// This section defines the logic to construct a graph from a borrow arrangement, performing check
// actions along the way.

fn is_writable(graph: &Graph<Loc, char>, r: Ref) -> bool {
    graph.is_mutable(r).unwrap()
        && graph
            .borrowed_by(r)
            .unwrap()
            .values()
            .all(|paths| paths.iter().all(|path| path.is_epsilon()))
}

type Loc = ();

fn build_and_check_graph_from_arrangement(
    arr: &BorrowArrangement,
) -> Result<(Graph<Loc, char>, BTreeMap<usize, Ref>), String> {
    let BorrowArrangement { locals, ops } = arr;

    // -------------------------------------------------------------------------
    // 1. Create locals as base references
    // -------------------------------------------------------------------------

    // Add all the locals to the graph as base "mutable references."
    let local_defs: Vec<_> = locals.iter().map(|i| (i, (), true)).collect::<Vec<_>>();

    let (mut g, local_map) = Graph::new(local_defs).unwrap();
    // Ensure we do not make new graphs
    let g_ref = &mut g;

    // Tracks where each reference is in the arrangement
    let mut refs = BTreeMap::new();

    // -------------------------------------------------------------------------
    // 2. Replay the arrangement sequentially
    // -------------------------------------------------------------------------
    for op in ops.iter() {
        match op {
            BorrowTestOp::Check { line, writables } => {
                let mut failures = Vec::new();
                for ref_ in &refs {
                    if writables.contains(ref_.0) {
                        if !is_writable(g_ref, *ref_.1) {
                            failures.push((true, ref_.0));
                        }
                    } else if is_writable(g_ref, *ref_.1) {
                        failures.push((false, ref_.0));
                    }
                }
                if !failures.is_empty() {
                    let failure_msg = failures
                        .into_iter()
                        .map(|(should_be_writable, idx)| {
                            if should_be_writable {
                                format!("  Ref {} is not writable", idx)
                            } else {
                                format!("  Ref {} is writable", idx)
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    return Err(format!(
                        "Writability check failed at line {line}:\n{failure_msg}"
                    ));
                }
            }
            BorrowTestOp::Assign { lhs, rhs } => match rhs {
                BorrowOp::Local {
                    local_id,
                    is_mutable,
                } => {
                    let base_ref = local_map[&{ *local_id }];
                    let new_ref = g_ref
                        .extend_by_epsilon((), [base_ref], *is_mutable)
                        .unwrap();
                    refs.insert(*lhs, new_ref);
                }
                BorrowOp::Alias { base, is_mutable } => {
                    let base_ref = refs[base];
                    let new_ref = g_ref
                        .extend_by_epsilon((), [base_ref], *is_mutable)
                        .unwrap();
                    refs.insert(*lhs, new_ref);
                }
                BorrowOp::BorrowField {
                    base,
                    label,
                    is_mutable,
                } => {
                    let base_ref = refs[base];
                    let new_ref = g_ref
                        .extend_by_label((), [base_ref], *is_mutable, *label)
                        .unwrap();
                    refs.insert(*lhs, new_ref);
                }
                BorrowOp::Call { args, is_mutable } => {
                    let arg_refs: Vec<_> = args.iter().map(|a| refs[a]).collect();
                    let muts = vec![*is_mutable];
                    let new_refs = g_ref
                        .extend_by_dot_star_for_call((), arg_refs, muts)
                        .unwrap();
                    assert!(new_refs.len() == 1);
                    refs.insert(*lhs, new_refs[0]);
                }
            },
        }
    }

    // -------------------------------------------------------------------------
    // 3. Return the constructed graph and the reference mapping
    // -------------------------------------------------------------------------
    Ok((g, refs))
}

// -----------------------------------------------
// Unit Test -- Sanity Check
// -----------------------------------------------

#[test]
fn test_simple_build_graph() {
    let op_0 = BorrowOp::Local {
        local_id: 0,
        is_mutable: true,
    };
    let op_1 = BorrowOp::BorrowField {
        base: 0,
        label: 'a',
        is_mutable: true,
    };
    let op_2 = BorrowOp::Call {
        args: vec![1],
        is_mutable: true,
    };

    let arr = BorrowArrangement {
        locals: BTreeSet::from([0u8]),
        ops: vec![
            BorrowTestOp::Assign { lhs: 0, rhs: op_0 }, // 0
            BorrowTestOp::Assign { lhs: 1, rhs: op_1 }, // 1
            BorrowTestOp::Assign { lhs: 2, rhs: op_2 }, // 2
            BorrowTestOp::Check {
                line: 3,
                writables: vec![2],
            },
        ],
    };

    let _ = build_and_check_graph_from_arrangement(&arr);
}

// -------------------------------------------------------------------------------------------------
// Display
// -------------------------------------------------------------------------------------------------

impl std::fmt::Debug for BorrowArrangement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "// {} locals", self.locals.len())?;
        for op in self.ops.iter() {
            match op {
                BorrowTestOp::Check { writables, .. } => {
                    writeln!(
                        f,
                        "writable: {}",
                        writables
                            .iter()
                            .map(|i| i.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )?;
                }
                BorrowTestOp::Assign { lhs, rhs } => {
                    let rhs_str = match rhs {
                        BorrowOp::Local {
                            local_id,
                            is_mutable,
                        } => format!(
                            "{} local_{}",
                            if *is_mutable { "&mut" } else { "&" },
                            local_id
                        ),
                        BorrowOp::Alias { base, is_mutable } => {
                            format!("{} {}", if *is_mutable { "&mut" } else { "&" }, base)
                        }
                        BorrowOp::BorrowField {
                            base,
                            label,
                            is_mutable,
                        } => format!(
                            "{} {}.{}",
                            if *is_mutable { "&mut" } else { "&" },
                            base,
                            label
                        ),
                        BorrowOp::Call { args, is_mutable } => format!(
                            "{} call({})",
                            if *is_mutable { "&mut" } else { "&" },
                            args.iter()
                                .map(|a| a.to_string())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    };
                    writeln!(f, "{} = {}", lhs, rhs_str)?;
                }
            }
        }
        Ok(())
    }
}

// -------------------------------------------------------------------------------------------------
// Test Harness
// -------------------------------------------------------------------------------------------------
// This section defines the test harness for running tests from files.

pub fn run_borrow_arrangement_test(file: &std::path::Path) -> Result<(), String> {
    let arrangement = BorrowArrangement::from_file(file)?;
    let (_graph, _refs) = build_and_check_graph_from_arrangement(&arrangement)?;
    Ok(())
}
