// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    access::ModuleAccess,
    control_flow_graph::VMControlFlowGraph,
    file_format::{AbilitySet, CodeUnit, FunctionDefinitionIndex, FunctionHandle, Signature},
    CompiledModule,
};

// A `FunctionView` holds all the information needed by the verifier for a
// `FunctionDefinition` and its `FunctionHandle` in a single view.
// A control flow graph is built for a function when the `FunctionView` is
// created.
// A `FunctionView` is created for all module functions except native functions.
// It is also created for a script.
pub struct FunctionView<'a> {
    index: Option<FunctionDefinitionIndex>,
    code: &'a CodeUnit,
    parameters: &'a Signature,
    return_: &'a Signature,
    locals: &'a Signature,
    type_parameters: &'a [AbilitySet],
    cfg: VMControlFlowGraph,
}

impl<'a> FunctionView<'a> {
    // Creates a `FunctionView` for a module function.
    pub fn function(
        module: &'a CompiledModule,
        index: FunctionDefinitionIndex,
        code: &'a CodeUnit,
        function_handle: &'a FunctionHandle,
    ) -> Self {
        Self {
            index: Some(index),
            code,
            parameters: module.signature_at(function_handle.parameters),
            return_: module.signature_at(function_handle.return_),
            locals: module.signature_at(code.locals),
            type_parameters: &function_handle.type_parameters,
            cfg: VMControlFlowGraph::new(&code.code),
        }
    }

    pub fn index(&self) -> Option<FunctionDefinitionIndex> {
        self.index
    }

    pub fn code(&self) -> &CodeUnit {
        self.code
    }

    pub fn parameters(&self) -> &Signature {
        self.parameters
    }

    pub fn return_(&self) -> &Signature {
        self.return_
    }

    pub fn locals(&self) -> &Signature {
        self.locals
    }

    pub fn type_parameters(&self) -> &[AbilitySet] {
        self.type_parameters
    }

    pub fn cfg(&self) -> &VMControlFlowGraph {
        &self.cfg
    }
}
