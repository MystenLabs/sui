// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{marking::MarkedSourceMapping, source_map::SourceMap};
use anyhow::Result;
use move_binary_format::CompiledModule;
use move_ir_types::location::Loc;

/// An object that associates source code with compiled bytecode and source map.
#[derive(Debug)]
pub struct SourceMapping<'a> {
    // The resulting bytecode from compiling the source map
    pub bytecode: &'a CompiledModule,

    // The source map for the bytecode made w.r.t. to the `source_code`
    pub source_map: SourceMap,

    // The source code for the bytecode. This is not required for disassembly, but it is required
    // for being able to print out corresponding source code for marked functions and structs.
    // Unused for now, this will be used when we start printing function/struct markings
    pub source_code: Option<(String, String)>,

    // Function and struct markings. These are used to lift up annotations/messages on the bytecode
    // into the disassembled program and/or IR source code.
    pub marks: Option<MarkedSourceMapping>,
}

impl<'a> SourceMapping<'a> {
    pub fn new(source_map: SourceMap, bytecode: &'a CompiledModule) -> Self {
        Self {
            source_map,
            bytecode,
            source_code: None,
            marks: None,
        }
    }

    pub fn new_without_source_map(bytecode: &'a CompiledModule, default_loc: Loc) -> Result<Self> {
        Ok(Self::new(
            SourceMap::dummy_from_view(bytecode, default_loc)?,
            bytecode,
        ))
    }

    pub fn with_marks(&mut self, marks: MarkedSourceMapping) {
        self.marks = Some(marks);
    }

    pub fn with_source_code(&mut self, source_code: (String, String)) {
        self.source_code = Some(source_code);
    }
}
