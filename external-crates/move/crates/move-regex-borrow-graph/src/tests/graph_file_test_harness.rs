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

impl BorrowOp {
    fn is_mutable(&self) -> bool {
        match self {
            BorrowOp::Local { is_mutable, .. } => *is_mutable,
            BorrowOp::Alias { is_mutable, .. } => *is_mutable,
            BorrowOp::BorrowField { is_mutable, .. } => *is_mutable,
            BorrowOp::Call { is_mutable, .. } => *is_mutable,
        }
    }
}

impl BorrowArrangement {
    fn from_file(file: &std::path::Path) -> Result<Self, String> {
        // Sanity check: that mutable set exists, and each mutable borrow is from a mutable base
        // or a local.
        fn sanity_check(ops: &[BorrowTestOp]) -> Result<(), String> {
            let mut mutable = BTreeSet::new();
            let mut defined = BTreeSet::new();

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

            for op in ops {
                match op {
                    BorrowTestOp::Check { writables, .. } => {
                        for m in writables {
                            test_assert!(
                                defined.contains(m),
                                "Invalid: expected writable {} not defined",
                                m
                            );
                            // test_assert!(
                            //     mutables.contains(m),
                            //     "Invalid: expected {} to be mutable",
                            //     m
                            // );
                        }
                    }
                    BorrowTestOp::Assign { lhs, rhs } => {
                        match rhs {
                            BorrowOp::Local { .. } | BorrowOp::Call { .. } => (),
                            BorrowOp::Alias { base, is_mutable } => {
                                test_assert!(
                                    !is_mutable || mutable.contains(base),
                                    "Invalud: alias {} mutable from immutable base {}",
                                    lhs,
                                    base
                                );
                            }
                            BorrowOp::BorrowField {
                                base, is_mutable, ..
                            } => {
                                test_assert!(
                                    !is_mutable || mutable.contains(base),
                                    "Invalid: mutable field borrow {} mutable from immutable base {}",
                                    lhs,
                                    base
                                );
                            }
                        }

                        test_assert!(
                            !defined.contains(lhs),
                            "Invalid: lhs {} already defined",
                            lhs
                        );
                        defined.insert(lhs);

                        if rhs.is_mutable() {
                            mutable.insert(lhs);
                        }
                    }
                }
            }
            Ok(())
        }

        // line := "writable: n,.."
        //       | "<ndx> = <ref> <base>"
        //       | "<ndx> = <ref> <base>.<label>"
        //       | "<ndx> = <ref> call(<arg1>, ...)"
        // base := <ndx> | "local_<local_id>"
        // ref  := "&" | "&mut"
        fn parse_line(line: usize, contents: &str) -> BorrowTestOp {
            if let Some(writables) = contents.strip_prefix("writable:") {
                let writables = writables.trim();
                // if no writables, then empty vec
                let writables = {
                    if writables.is_empty() {
                        vec![]
                    } else {
                        writables
                            .split(',')
                            .map(|s| s.trim().parse().expect("Invalid mutable index"))
                            .collect()
                    }
                };
                return BorrowTestOp::Check { line, writables };
            }

            let parts: Vec<&str> = contents.split('=').map(|s| s.trim()).collect();
            assert!(parts.len() == 2, "Invalid line format");
            let lhs = parts[0];
            let rhs = parts[1];

            let lhs: usize = lhs.parse().expect("Invalid index on LHS");

            let rhs = {
                let rhs_parts: Vec<&str> = rhs.split_whitespace().collect();
                assert!(rhs_parts.len() >= 2, "Invalid RHS format");
                let ref_kind = rhs_parts[0];
                let is_mutable = match ref_kind {
                    "&" => false,
                    "&mut" => true,
                    _ => panic!("Invalid reference kind"),
                };
                let target = rhs_parts[1];

                if let Some(local_id) = target.strip_prefix("local_") {
                    let local_id: u8 = local_id.parse().expect("Invalid local id");
                    BorrowOp::Local {
                        local_id,
                        is_mutable,
                    }
                } else if target.contains(".") {
                    let base_and_field: Vec<&str> = target.split('.').collect();
                    assert!(base_and_field.len() == 2, "Invalid field borrow format");
                    let base: usize = base_and_field[0]
                        .parse()
                        .expect("Invalid base index for field borrow");
                    let label: char = base_and_field[1]
                        .chars()
                        .next()
                        .expect("Invalid label for field borrow");
                    BorrowOp::BorrowField {
                        base,
                        label,
                        is_mutable,
                    }
                } else if let Some(args) = target.strip_prefix("call") {
                    let args_str = args.trim_matches(|c| c == '(' || c == ')');
                    let args: Vec<usize> = if args_str.is_empty() {
                        Vec::new()
                    } else {
                        args_str
                            .split(',')
                            .map(|s| s.trim().parse().expect("Invalid call argument"))
                            .collect()
                    };
                    BorrowOp::Call { args, is_mutable }
                } else {
                    let base: usize = target.parse().expect("Invalid base index for alias");
                    BorrowOp::Alias { base, is_mutable }
                }
            };

            BorrowTestOp::Assign { lhs, rhs }
        }

        let content = std::fs::read_to_string(file).expect("Unable to read file");
        let lines = content.lines();

        // Subsequent lines: operations
        let mut ops = Vec::new();
        for (ndx, line) in lines.enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("//") {
                continue;
            }

            let op = parse_line(ndx + 1, line);
            // Parse operation
            ops.push(op);
        }

        if ops.is_empty() {
            panic!("No operations found in file");
        }

        sanity_check(&ops)?;

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
                        "Writability check failed as line {line}:\n{failure_msg}"
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
