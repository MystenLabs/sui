// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use lcov::record::Record as LRecord;
use move_abstract_interpreter::control_flow_graph::ControlFlowGraph;
use move_binary_format::file_format::FunctionDefinitionIndex;
use move_bytecode_verifier::absint::VMControlFlowGraph;
use move_compiler::{
    compiled_unit::CompiledUnit, shared::files::MappedFiles,
    unit_test::filter_test_members::UNIT_TEST_POISON_FUN_NAME,
};
use move_core_types::language_storage::ModuleId;
use move_trace_format::format::{MoveTraceReader, TraceEvent};
use std::fmt::Write;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::PathBuf,
};

pub type LineNumber = usize;
pub type HitCount = usize;
pub type BranchNumber = u16;
pub type BlockNumber = usize;

// A coverage record keeper for a package.
pub struct PackageRecordKeeper {
    pub file_record_keepers: BTreeMap<ModuleId, FileRecordKeeper>,
    // File mapping. Has source files + ability to remap `Loc`s to a source line number.
    pub file_mapping: MappedFiles,
}

// A coverage record keeper for a single file.
pub struct FileRecordKeeper {
    // Path to source file (or generated disassembl file if source is not available)
    pub source_file_path: PathBuf,
    // Functions that exist
    pub functions_found: BTreeMap<String, LineNumber>,
    // Lines that are instrumented (that appear in bytecode)
    pub instrumented_lines: BTreeSet<LineNumber>,

    // Trace-dependent fields

    // The line entries (line number, hit count)
    pub line_entries: BTreeMap<LineNumber, HitCount>,
    // Functions called
    pub functions_hit: BTreeMap<String, HitCount>,

    // Bookeeping state
    unit: CompiledUnit,
    branches: BTreeMap<(u16, u16), BranchInfo>,
}

#[derive(Debug, Clone)]
struct BranchInfo {
    line_no: LineNumber,
    block_id: BlockNumber,
    branches: BTreeMap<BranchNumber, HitCount>,
}

impl BranchInfo {
    pub fn new(line_no: LineNumber, block_id: BlockNumber) -> Self {
        Self {
            line_no,
            block_id,
            branches: BTreeMap::new(),
        }
    }

    pub fn add_branch(&mut self, branch: BranchNumber) {
        self.branches.insert(branch, 0);
    }

    pub fn hit_branch(&mut self, branch: BranchNumber) {
        if let Some(h) = self.branches.get_mut(&branch) {
            *h += 1;
        }
    }
}

impl PackageRecordKeeper {
    pub fn new(units: Vec<(CompiledUnit, PathBuf)>, file_mapping: MappedFiles) -> Self {
        let mut file_record_keepers = BTreeMap::new();
        for (unit, source_path) in units.into_iter() {
            let module_id = unit.module.self_id();
            let record = FileRecordKeeper::new(unit, source_path, &file_mapping);
            file_record_keepers.insert(module_id, record);
        }
        Self {
            file_record_keepers,
            file_mapping,
        }
    }

    pub fn lcov_record_string(&self) -> String {
        self.file_record_keepers
            .iter()
            .flat_map(|r| r.1.to_lcov_records())
            .fold(String::new(), |mut acc, r| {
                writeln!(acc, "{r}").unwrap();
                acc
            })
    }

    // Build up the functions hit, executed lines, and branches hit.
    pub fn calculate_coverage<R: Read>(&mut self, trace: MoveTraceReader<'_, R>) {
        let mut current_fn_index = vec![];
        let mut current_record_id = vec![];
        let mut coming_from = None;

        for event in trace {
            match event.unwrap() {
                TraceEvent::OpenFrame { frame, .. } => {
                    let module_id = frame.module.clone();
                    let record = self.file_record_keepers.get_mut(&module_id).unwrap();
                    let name = frame.function_name.clone();
                    record
                        .functions_hit
                        .entry(name.clone())
                        .and_modify(|e| *e += 1)
                        .or_insert(1);
                    let Ok(smap) =
                        record
                            .unit
                            .source_map
                            .get_function_source_map(FunctionDefinitionIndex(
                                frame.binary_member_index,
                            ))
                    else {
                        continue;
                    };
                    let line = self
                        .file_mapping
                        .start_position(&smap.definition_location)
                        .line_offset()
                        + 1;
                    record
                        .line_entries
                        .entry(line)
                        .and_modify(|e| *e += 1)
                        .or_insert(1);
                    current_fn_index.push(frame.binary_member_index);
                    current_record_id.push(module_id);
                    coming_from = None;
                }
                TraceEvent::Instruction { pc, .. } => {
                    let module_id = current_record_id.last().unwrap();
                    let current_fn_index = current_fn_index.last().unwrap();
                    let record = self.file_record_keepers.get_mut(module_id).unwrap();
                    let Ok(loc) = record
                        .unit
                        .source_map
                        .get_code_location(FunctionDefinitionIndex(*current_fn_index), pc)
                    else {
                        continue;
                    };
                    let line = self.file_mapping.start_position(&loc).line_offset() + 1;
                    debug_assert!(record.instrumented_lines.contains(&line));

                    record
                        .line_entries
                        .entry(line)
                        .and_modify(|e| *e += 1)
                        .or_insert(1);

                    if let Some(from) = coming_from {
                        if let Some(info) = record.branches.get_mut(&(*current_fn_index, from)) {
                            info.hit_branch(pc)
                        }
                    }

                    coming_from = None;

                    let branch_info_opt = record.branches.get_mut(&(*current_fn_index, pc));

                    if branch_info_opt.is_some() {
                        coming_from = Some(pc);
                    }
                }
                TraceEvent::CloseFrame { .. } => {
                    current_fn_index.pop();
                    current_record_id.pop();
                    coming_from = None;
                }
                TraceEvent::Effect(_) | TraceEvent::External(_) => (),
            }
        }
    }
}

impl FileRecordKeeper {
    pub fn new(unit: CompiledUnit, source_path: PathBuf, file_mapping: &MappedFiles) -> Self {
        let functions_found = BTreeMap::new();
        let instrumented_lines = BTreeSet::new();

        let functions_hit = BTreeMap::new();
        let line_entries = BTreeMap::new();

        let branches = BTreeMap::new();

        let mut information = Self {
            source_file_path: source_path.canonicalize().unwrap(),
            functions_found,
            functions_hit,
            instrumented_lines,
            line_entries,
            unit,
            branches,
        };
        information.populate_info_fields(file_mapping);
        information
    }

    pub fn to_lcov_records(&self) -> Vec<LRecord> {
        let FileRecordKeeper {
            source_file_path,
            functions_found,
            instrumented_lines,
            line_entries,
            functions_hit,
            unit: _,
            branches,
        } = self;

        let mut records = vec![
            LRecord::SourceFile {
                path: source_file_path.clone(),
            },
            LRecord::FunctionsFound {
                found: functions_found.len() as u32,
            },
            LRecord::FunctionsHit {
                hit: functions_hit.len() as u32,
            },
            LRecord::LinesFound {
                found: instrumented_lines.len() as u32,
            },
            LRecord::LinesHit {
                hit: line_entries.len() as u32,
            },
            LRecord::BranchesFound {
                found: branches
                    .values()
                    .map(|info| info.branches.len())
                    .sum::<usize>() as u32,
            },
            LRecord::BranchesHit {
                hit: branches
                    .values()
                    .map(|info| info.branches.values().filter(|hi| **hi > 0).count())
                    .sum::<usize>() as u32,
            },
        ];

        for (function_name, &start_line) in functions_found {
            records.push(LRecord::FunctionName {
                name: function_name.to_owned(),
                start_line: start_line as u32,
            });
        }

        for (function_name, &hit_count) in functions_hit {
            records.push(LRecord::FunctionData {
                name: function_name.to_owned(),
                count: hit_count as u64,
            });
        }

        for (&line_number, &hit_count) in line_entries {
            records.push(LRecord::LineData {
                line: line_number as u32,
                count: hit_count as u64,
                checksum: None,
            });
        }

        for BranchInfo {
            line_no,
            block_id,
            branches,
        } in branches.values()
        {
            for (&branch_id, &hit_count) in branches {
                records.push(LRecord::BranchData {
                    line: *line_no as u32,
                    block: *block_id as u32,
                    branch: branch_id as u32,
                    taken: if hit_count == 0 {
                        None
                    } else {
                        Some(hit_count as u64)
                    },
                });
            }
        }

        records.push(LRecord::EndOfRecord);

        records
    }

    fn build_control_flow_graph(&self, function_index: u16) -> VMControlFlowGraph {
        let fdef = self
            .unit
            .module
            .function_def_at(FunctionDefinitionIndex(function_index));
        let code = fdef
            .code
            .as_ref()
            .expect("Should only be called on non-native funs");
        VMControlFlowGraph::new(&code.code, &code.jump_tables)
    }

    // Build up the functions found, instrumented lines, and branches found.
    fn populate_info_fields(&mut self, file_mapping: &MappedFiles) {
        let mut block_id = 0;
        for (index, fdef) in self.unit.module.function_defs().iter().enumerate() {
            let name = self
                .unit
                .module
                .identifier_at(self.unit.module.function_handle_at(fdef.function).name)
                .to_string();

            if UNIT_TEST_POISON_FUN_NAME.as_str() == name {
                continue;
            }

            let f_source_map = self
                .unit
                .source_map
                .get_function_source_map(FunctionDefinitionIndex(index as u16))
                .unwrap();
            let f_line_no = file_mapping
                .start_position(&f_source_map.definition_location)
                .line_offset()
                + 1;

            self.functions_found.insert(name, f_line_no + 1);
            self.instrumented_lines.insert(f_line_no);

            if let Some(code) = &fdef.code {
                for (pc, _) in code.code.iter().enumerate() {
                    let Some(loc) = f_source_map.get_code_location(pc as u16) else {
                        continue;
                    };
                    let line_no = file_mapping.start_position(&loc).line_offset() + 1;
                    self.instrumented_lines.insert(line_no);
                }

                let cfg = self.build_control_flow_graph(index as u16);
                for cfg_block_id in cfg.blocks() {
                    let block_end = cfg.block_end(cfg_block_id);
                    let loc = f_source_map.get_code_location(block_end).unwrap();
                    let line_no = file_mapping.start_position(&loc).line_offset() + 1;
                    block_id += 1;
                    for o in cfg.successors(cfg_block_id) {
                        self.branches
                            .entry((index as u16, block_end))
                            .or_insert_with(|| BranchInfo::new(line_no, block_id - 1))
                            .add_branch(o);
                    }
                }
            }
        }
    }
}
