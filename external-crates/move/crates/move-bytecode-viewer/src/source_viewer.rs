// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use anyhow::Result;
use move_bytecode_source_map::source_map::SourceMap;

use crate::{
    bytecode_viewer::{BytecodeInfo, BytecodeViewer},
    interfaces::{RightScreen, SourceContext},
};
use move_binary_format::file_format::CompiledModule;
use std::{cmp, fs, path::Path};

const CONTEXT_SIZE: usize = 1000;

#[derive(Debug, Clone)]
pub struct ModuleViewer {
    file_index: usize,
    source_code: Vec<String>,
    source_map: SourceMap,
    #[allow(unused)]
    module: CompiledModule,
}

impl ModuleViewer {
    pub fn new(module: CompiledModule, source_map: SourceMap, source_location: &Path) -> Self {
        let mut source_code = vec![];
        let file_contents = fs::read_to_string(source_location).unwrap();
        assert!(
            source_map.check(&file_contents),
            "File contents are out of sync with source map"
        );
        source_code.push(file_contents);
        let file_index = source_code.len() - 1;

        Self {
            file_index,
            source_code,
            source_map,
            module,
        }
    }
}

impl<'a> RightScreen<BytecodeViewer<'a>> for ModuleViewer {
    fn source_for_code_location(&self, bytecode_info: &BytecodeInfo) -> Result<SourceContext> {
        let loc = self
            .source_map
            .get_code_location(bytecode_info.function_index, bytecode_info.code_offset)?;

        let loc_start = loc.start() as usize;
        let loc_end = loc.end() as usize;
        let source = &self.source_code[self.file_index];
        let source_span_end = source.len();

        // Determine the context around the span that we want to show.
        // Show either from the start of the file, or back `CONTEXT_SIZE` number of characters.
        let context_start = loc_start.saturating_sub(CONTEXT_SIZE);
        // Show either to the end of the file, or `CONTEXT_SIZE` number of characters after the
        // span end.
        let context_end = cmp::min(loc_end.checked_add(CONTEXT_SIZE).unwrap(), source_span_end);

        // Create the contexts
        let left = &source[context_start..loc_start];
        let highlight = source[loc_start..loc_end].to_owned();
        let remainder = &source[loc_end..context_end];

        Ok(SourceContext {
            left: left.to_string(),
            highlight,
            remainder: remainder.to_string(),
        })
    }

    fn backing_string(&self) -> String {
        self.source_code[self.file_index].clone()
    }
}
