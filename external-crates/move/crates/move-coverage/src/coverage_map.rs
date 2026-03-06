// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use anyhow::{Result, format_err};
use move_binary_format::file_format::{CodeOffset, CompiledModule};
use move_core_types::{
    account_address::AccountAddress,
    identifier::{IdentStr, Identifier},
};
use move_trace_format::format::{MoveTraceReader, TraceEvent};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt::Debug,
    fs::File,
    io::{Read, Write},
    path::Path,
};

pub type FunctionCoverage = BTreeMap<u64, u64>;

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct CoverageMap {
    pub exec_maps: BTreeMap<String, ExecCoverageMap>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ModuleCoverageMap {
    pub module_addr: AccountAddress,
    pub module_name: Identifier,
    pub function_maps: BTreeMap<Identifier, FunctionCoverage>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecCoverageMap {
    pub exec_id: String,
    pub module_maps: BTreeMap<(AccountAddress, Identifier), ModuleCoverageMap>,
}

#[derive(Debug)]
pub struct ExecCoverageMapWithModules {
    pub module_maps: BTreeMap<(String, AccountAddress, Identifier), ModuleCoverageMap>,
    pub compiled_modules: BTreeMap<String, CompiledModule>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TraceEntry {
    pub module_addr: AccountAddress,
    pub module_name: Identifier,
    pub func_name: Identifier,
    pub func_pc: CodeOffset,
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct TraceMap {
    pub exec_maps: BTreeMap<String, Vec<TraceEntry>>,
}

/// Trait for types that consume Move VM trace events. Implementors only need to provide
/// `record_instruction`; directory iteration, file reading, and trace event walking are
/// provided as default methods.
pub trait TraceConsumer: Default {
    fn record_instruction(
        &mut self,
        test_name: &str,
        module_addr: AccountAddress,
        module_name: Identifier,
        func_name: Identifier,
        pc: u64,
    );

    fn ingest_trace<R: Read>(
        mut self,
        test_name: &str,
        trace_reader: MoveTraceReader<'_, R>,
    ) -> Self {
        let mut current_fn_context = vec![];
        for event in trace_reader {
            match event.unwrap() {
                TraceEvent::Effect(_) | TraceEvent::External(_) => (),
                TraceEvent::OpenFrame { frame, .. } => {
                    current_fn_context.push(frame);
                }
                TraceEvent::CloseFrame { .. } => {
                    current_fn_context.pop().unwrap();
                }
                TraceEvent::Instruction { pc, .. } => {
                    let current_frame = current_fn_context.last().unwrap();
                    self.record_instruction(
                        test_name,
                        *current_frame.module.address(),
                        current_frame.module.name().to_owned(),
                        Identifier::new(current_frame.function_name.clone()).unwrap(),
                        pc as u64,
                    );
                }
            }
        }
        self
    }

    fn ingest_trace_dir<P: AsRef<Path> + Debug>(mut self, dirname: P) -> Self {
        for entry in std::fs::read_dir(&dirname).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_file() {
                let file = File::open(&path)
                    .unwrap_or_else(|e| panic!("Unable to open trace file '{:?}': {}", path, e));
                let move_trace_reader =
                    MoveTraceReader::new(file).expect("Unable to read trace file");
                let test_name = path.file_name().unwrap().to_str().unwrap();
                let test_name = test_name.replace("__", "::");
                self = self.ingest_trace(&test_name, move_trace_reader);
            }
        }
        self
    }

    fn from_trace_dir<P: AsRef<Path> + Debug>(dirname: P) -> Self {
        Self::default().ingest_trace_dir(dirname)
    }
}

impl TraceConsumer for CoverageMap {
    fn record_instruction(
        &mut self,
        test_name: &str,
        module_addr: AccountAddress,
        module_name: Identifier,
        func_name: Identifier,
        pc: u64,
    ) {
        self.insert(test_name, module_addr, module_name, func_name, pc);
    }
}

impl TraceConsumer for TraceMap {
    fn record_instruction(
        &mut self,
        test_name: &str,
        module_addr: AccountAddress,
        module_name: Identifier,
        func_name: Identifier,
        pc: u64,
    ) {
        self.insert(test_name, module_addr, module_name, func_name, pc);
    }
}

impl CoverageMap {
    /// Takes in a file containing a serialized coverage map and returns a coverage map.
    pub fn from_binary_file<P: AsRef<Path> + std::fmt::Debug>(filename: P) -> Result<Self> {
        let mut bytes = Vec::new();
        File::open(&filename)
            .map_err(|e| format_err!("{}: Coverage map file '{:?}' doesn't exist", e, filename))?
            .read_to_end(&mut bytes)
            .ok()
            .ok_or_else(|| format_err!("Unable to read coverage map"))?;
        bcs::from_bytes(&bytes).map_err(|_| format_err!("Error deserializing coverage map"))
    }

    // add entries in a cascading manner
    pub fn insert(
        &mut self,
        exec_id: &str,
        module_addr: AccountAddress,
        module_name: Identifier,
        func_name: Identifier,
        pc: u64,
    ) {
        let exec_entry = self
            .exec_maps
            .entry(exec_id.to_owned())
            .or_insert_with(|| ExecCoverageMap::new(exec_id.to_owned()));
        exec_entry.insert(module_addr, module_name, func_name, pc);
    }

    pub fn to_unified_exec_map(&self) -> ExecCoverageMap {
        let mut unified_map = ExecCoverageMap::new(String::new());
        for (_, exec_map) in self.exec_maps.iter() {
            for ((module_addr, module_name), module_map) in exec_map.module_maps.iter() {
                for (func_name, func_map) in module_map.function_maps.iter() {
                    for (pc, count) in func_map.iter() {
                        unified_map.insert_multi(
                            *module_addr,
                            module_name.clone(),
                            func_name.clone(),
                            *pc,
                            *count,
                        );
                    }
                }
            }
        }
        unified_map
    }
}

impl ModuleCoverageMap {
    pub fn new(module_addr: AccountAddress, module_name: Identifier) -> Self {
        ModuleCoverageMap {
            module_addr,
            module_name,
            function_maps: BTreeMap::new(),
        }
    }

    pub fn insert_multi(&mut self, func_name: Identifier, pc: u64, count: u64) {
        let func_entry = self.function_maps.entry(func_name).or_default();
        let pc_entry = func_entry.entry(pc).or_insert(0);
        *pc_entry += count;
    }

    pub fn insert(&mut self, func_name: Identifier, pc: u64) {
        self.insert_multi(func_name, pc, 1);
    }

    pub fn merge(&mut self, another: ModuleCoverageMap) {
        for (key, val) in another.function_maps {
            self.function_maps.entry(key).or_default().extend(val);
        }
    }

    pub fn get_function_coverage(&self, func_name: &IdentStr) -> Option<&FunctionCoverage> {
        self.function_maps.get(func_name)
    }
}

impl ExecCoverageMap {
    pub fn new(exec_id: String) -> Self {
        ExecCoverageMap {
            exec_id,
            module_maps: BTreeMap::new(),
        }
    }

    pub fn insert_multi(
        &mut self,
        module_addr: AccountAddress,
        module_name: Identifier,
        func_name: Identifier,
        pc: u64,
        count: u64,
    ) {
        let module_entry = self
            .module_maps
            .entry((module_addr, module_name.clone()))
            .or_insert_with(|| ModuleCoverageMap::new(module_addr, module_name));
        module_entry.insert_multi(func_name, pc, count);
    }

    pub fn insert(
        &mut self,
        module_addr: AccountAddress,
        module_name: Identifier,
        func_name: Identifier,
        pc: u64,
    ) {
        self.insert_multi(module_addr, module_name, func_name, pc, 1);
    }

    pub fn into_coverage_map_with_modules(
        self,
        modules: BTreeMap<AccountAddress, BTreeMap<Identifier, (String, CompiledModule)>>,
    ) -> ExecCoverageMapWithModules {
        let retained: BTreeMap<(String, AccountAddress, Identifier), ModuleCoverageMap> = self
            .module_maps
            .into_iter()
            .filter_map(|((module_addr, module_name), module_cov)| {
                modules.get(&module_addr).and_then(|func_map| {
                    func_map.get(&module_name).map(|(module_path, _)| {
                        ((module_path.clone(), module_addr, module_name), module_cov)
                    })
                })
            })
            .collect();

        let compiled_modules = modules
            .into_iter()
            .flat_map(|(_, module_map)| {
                module_map
                    .into_iter()
                    .map(|(_, (module_path, compiled_module))| (module_path, compiled_module))
            })
            .collect();

        ExecCoverageMapWithModules {
            module_maps: retained,
            compiled_modules,
        }
    }
}

impl ExecCoverageMapWithModules {
    pub fn empty() -> Self {
        Self {
            module_maps: BTreeMap::new(),
            compiled_modules: BTreeMap::new(),
        }
    }

    pub fn merge(&mut self, another: ExecCoverageMapWithModules) {
        for ((module_path, module_addr, module_name), val) in another.module_maps {
            self.module_maps
                .entry((module_path.clone(), module_addr, module_name.clone()))
                .or_insert_with(|| ModuleCoverageMap::new(module_addr, module_name))
                .merge(val);
        }

        for (module_path, compiled_module) in another.compiled_modules {
            self.compiled_modules
                .entry(module_path)
                .or_insert(compiled_module);
        }
    }
}

impl TraceMap {
    // Takes in a file containing a serialized trace and deserialize it.
    pub fn from_binary_file<P: AsRef<Path>>(filename: P) -> Self {
        let mut bytes = Vec::new();
        File::open(filename)
            .ok()
            .and_then(|mut file| file.read_to_end(&mut bytes).ok())
            .ok_or_else(|| format_err!("Error while reading in coverage map binary"))
            .unwrap();
        bcs::from_bytes(&bytes)
            .map_err(|_| format_err!("Error deserializing into coverage map"))
            .unwrap()
    }

    // add entries in a cascading manner
    pub fn insert(
        &mut self,
        exec_id: &str,
        module_addr: AccountAddress,
        module_name: Identifier,
        func_name: Identifier,
        pc: u64,
    ) {
        let exec_entry = self.exec_maps.entry(exec_id.to_owned()).or_default();
        exec_entry.push(TraceEntry {
            module_addr,
            module_name,
            func_name,
            func_pc: pc as CodeOffset,
        });
    }
}

pub fn output_map_to_file<M: Serialize, P: AsRef<Path>>(file_name: P, data: &M) -> Result<()> {
    let bytes = bcs::to_bytes(data)?;
    let mut file = File::create(file_name)?;
    file.write_all(&bytes)?;
    Ok(())
}
