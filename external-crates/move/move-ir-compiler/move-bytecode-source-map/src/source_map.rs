// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{format_err, Result};
use move_binary_format::{
    access::ModuleAccess,
    binary_views::BinaryIndexedView,
    file_format::{
        AbilitySet, CodeOffset, CodeUnit, ConstantPoolIndex, FunctionDefinitionIndex, LocalIndex,
        MemberCount, ModuleHandleIndex, SignatureIndex, StructDefinition, StructDefinitionIndex,
        TableIndex,
    },
};
use move_command_line_common::files::FileHash;
use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use move_ir_types::{
    ast::{ConstantName, ModuleIdent, ModuleName, NopLabel},
    location::Loc,
};
use move_symbol_pool::Symbol;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, ops::Bound};

//***************************************************************************
// Source location mapping
//***************************************************************************

pub type SourceName = (String, Loc);

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StructSourceMap {
    /// The source declaration location of the struct
    pub definition_location: Loc,

    /// Important: type parameters need to be added in the order of their declaration
    pub type_parameters: Vec<SourceName>,

    /// Note that fields to a struct source map need to be added in the order of the fields in the
    /// struct definition.
    pub fields: Vec<Loc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FunctionSourceMap {
    /// The source location for the definition of this entire function. Note that in certain
    /// instances this will have no valid source location e.g. the "main" function for modules that
    /// are treated as programs are synthesized and therefore have no valid source location.
    pub definition_location: Loc,

    /// The names of the type parameters to the function.
    /// Note that type parameters need to be added in the order of their declaration.
    pub type_parameters: Vec<SourceName>,

    /// The names of the parameters to the function.
    pub parameters: Vec<SourceName>,

    /// The index into the vector is the local's index. The corresponding `(Identifier, Location)` tuple
    /// is the name and location of the local.
    pub locals: Vec<SourceName>,

    /// A map to the code offset for a corresponding nop. Nop's are used as markers for some
    /// high level language information
    pub nops: BTreeMap<NopLabel, CodeOffset>,

    /// The source location map for the function body.
    pub code_map: BTreeMap<CodeOffset, Loc>,

    /// Whether this function is a native function or not.
    pub is_native: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SourceMap {
    /// The source location for the definition of the module or script that this source map is for.
    pub definition_location: Loc,

    /// The name <address.module_name> of the module that this source map is for.
    /// `None` if this source map corresponds to a script.
    pub module_name_opt: Option<(AccountAddress, Identifier)>,

    // A mapping of `StructDefinitionIndex` to source map for each struct/resource.
    struct_map: BTreeMap<TableIndex, StructSourceMap>,

    // A mapping of `FunctionDefinitionIndex` to the soure map for that function.
    // For scripts, this map has a single element that points to a source map corresponding to the
    // script's "main" function.
    function_map: BTreeMap<TableIndex, FunctionSourceMap>,

    // A mapping of constant name to its `ConstantPoolIndex`.
    pub constant_map: BTreeMap<ConstantName, TableIndex>,
}

impl StructSourceMap {
    pub fn new(definition_location: Loc) -> Self {
        Self {
            definition_location,
            type_parameters: Vec::new(),
            fields: Vec::new(),
        }
    }

    pub fn add_type_parameter(&mut self, type_name: SourceName) {
        self.type_parameters.push(type_name)
    }

    pub fn get_type_parameter_name(&self, type_parameter_idx: usize) -> Option<SourceName> {
        self.type_parameters.get(type_parameter_idx).cloned()
    }

    pub fn add_field_location(&mut self, field_loc: Loc) {
        self.fields.push(field_loc)
    }

    pub fn get_field_location(&self, field_index: MemberCount) -> Option<Loc> {
        self.fields.get(field_index as usize).cloned()
    }

    pub fn dummy_struct_map(
        &mut self,
        view: &BinaryIndexedView,
        struct_def: &StructDefinition,
        default_loc: Loc,
    ) -> Result<()> {
        let struct_handle = view.struct_handle_at(struct_def.struct_handle);

        // Add dummy locations for the fields
        match struct_def.declared_field_count() {
            Err(_) => (),
            Ok(count) => (0..count).for_each(|_| self.fields.push(default_loc)),
        }

        for i in 0..struct_handle.type_parameters.len() {
            let name = format!("Ty{}", i);
            self.add_type_parameter((name, default_loc))
        }
        Ok(())
    }
}

impl FunctionSourceMap {
    pub fn new(definition_location: Loc, is_native: bool) -> Self {
        Self {
            definition_location,
            type_parameters: Vec::new(),
            parameters: Vec::new(),
            locals: Vec::new(),
            code_map: BTreeMap::new(),
            is_native,
            nops: BTreeMap::new(),
        }
    }

    pub fn add_type_parameter(&mut self, type_name: SourceName) {
        self.type_parameters.push(type_name)
    }

    pub fn get_type_parameter_name(&self, type_parameter_idx: usize) -> Option<SourceName> {
        self.type_parameters.get(type_parameter_idx).cloned()
    }

    /// A single source-level instruction may possibly map to a number of bytecode instructions. In
    /// order to not store a location for each instruction, we instead use a BTreeMap to represent
    /// a segment map (holding the left-hand-sides of each segment).  Thus, an instruction
    /// sequence is always marked from its starting point. To determine what part of the source
    /// code corresponds to a given `CodeOffset` we query to find the element that is the largest
    /// number less than or equal to the query. This will give us the location for that bytecode
    /// range.
    pub fn add_code_mapping(&mut self, start_offset: CodeOffset, location: Loc) {
        let possible_segment = self.get_code_location(start_offset);
        match possible_segment.map(|other_location| other_location != location) {
            Some(true) | None => {
                self.code_map.insert(start_offset, location);
            }
            _ => (),
        };
    }

    /// Record the code offset for an Nop label
    pub fn add_nop_mapping(&mut self, label: NopLabel, offset: CodeOffset) {
        assert!(self.nops.insert(label, offset).is_none())
    }

    // Note that it is important that locations be added in order.
    pub fn add_local_mapping(&mut self, name: SourceName) {
        self.locals.push(name);
    }

    pub fn add_parameter_mapping(&mut self, name: SourceName) {
        self.parameters.push(name)
    }

    /// Recall that we are using a segment tree. We therefore lookup the location for the code
    /// offset by performing a range query for the largest number less than or equal to the code
    /// offset passed in.
    pub fn get_code_location(&self, code_offset: CodeOffset) -> Option<Loc> {
        // If the function is a native, and we are asking for the "first bytecode offset in it"
        // return the location of the declaration of the function. Otherwise, we will return
        // `None`.
        if self.is_native {
            if code_offset == 0 {
                Some(self.definition_location)
            } else {
                None
            }
        } else {
            self.code_map
                .range((Bound::Unbounded, Bound::Included(&code_offset)))
                .next_back()
                .map(|(_, vl)| *vl)
        }
    }

    pub fn get_parameter_or_local_name(&self, idx: u64) -> Option<SourceName> {
        let idx = idx as usize;
        if idx < self.parameters.len() {
            self.parameters.get(idx).cloned()
        } else {
            self.locals.get(idx - self.parameters.len()).cloned()
        }
    }

    pub fn make_local_name_to_index_map(&self) -> BTreeMap<&String, LocalIndex> {
        self.parameters
            .iter()
            .chain(&self.locals)
            .enumerate()
            .map(|(i, (n, _))| (n, i as LocalIndex))
            .collect()
    }

    pub fn dummy_function_map(
        &mut self,
        view: &BinaryIndexedView,
        type_parameters: &[AbilitySet],
        parameters: SignatureIndex,
        code: Option<CodeUnit>,
        default_loc: Loc,
    ) -> Result<()> {
        // Generate names for each type parameter
        for i in 0..type_parameters.len() {
            let name = format!("Ty{}", i);
            self.add_type_parameter((name, default_loc))
        }

        // Generate names for each parameter
        let params = view.signature_at(parameters);
        for i in 0..params.0.len() {
            let name = format!("Arg{}", i);
            self.add_parameter_mapping((name, default_loc))
        }

        if let Some(code) = code {
            let locals = view.signature_at(code.locals);
            for i in 0..locals.0.len() {
                let name = format!("loc{}", i);
                self.add_local_mapping((name, default_loc))
            }
        }

        // We just need to insert the code map at the 0'th index since we represent this with a
        // segment map
        self.add_code_mapping(0, default_loc);

        Ok(())
    }
}

impl SourceMap {
    pub fn new(definition_location: Loc, module_name_opt: Option<ModuleIdent>) -> Self {
        let module_name_opt = module_name_opt.map(|module_name| {
            let ident = Identifier::new(module_name.name.0.as_str()).unwrap();
            (module_name.address, ident)
        });
        Self {
            definition_location,
            module_name_opt,
            struct_map: BTreeMap::new(),
            function_map: BTreeMap::new(),
            constant_map: BTreeMap::new(),
        }
    }

    pub fn check(&self, file_contents: &str) -> bool {
        let file_hash = FileHash::new(file_contents);
        self.definition_location.file_hash() == file_hash
    }

    pub fn add_top_level_function_mapping(
        &mut self,
        fdef_idx: FunctionDefinitionIndex,
        location: Loc,
        is_native: bool,
    ) -> Result<()> {
        self.function_map.insert(fdef_idx.0, FunctionSourceMap::new(location, is_native)).map_or(Ok(()), |_| { Err(format_err!(
                    "Multiple functions at same function definition index encountered when constructing source map"
                )) })
    }

    pub fn add_function_type_parameter_mapping(
        &mut self,
        fdef_idx: FunctionDefinitionIndex,
        name: SourceName,
    ) -> Result<()> {
        let func_entry = self.function_map.get_mut(&fdef_idx.0).ok_or_else(|| {
            format_err!("Tried to add function type parameter mapping to undefined function index")
        })?;
        func_entry.add_type_parameter(name);
        Ok(())
    }

    pub fn get_function_type_parameter_name(
        &self,
        fdef_idx: FunctionDefinitionIndex,
        type_parameter_idx: usize,
    ) -> Result<SourceName> {
        self.function_map
            .get(&fdef_idx.0)
            .and_then(|function_source_map| {
                function_source_map.get_type_parameter_name(type_parameter_idx)
            })
            .ok_or_else(|| format_err!("Unable to get function type parameter name"))
    }

    pub fn add_code_mapping(
        &mut self,
        fdef_idx: FunctionDefinitionIndex,
        start_offset: CodeOffset,
        location: Loc,
    ) -> Result<()> {
        let func_entry = self
            .function_map
            .get_mut(&fdef_idx.0)
            .ok_or_else(|| format_err!("Tried to add code mapping to undefined function index"))?;
        func_entry.add_code_mapping(start_offset, location);
        Ok(())
    }

    pub fn add_nop_mapping(
        &mut self,
        fdef_idx: FunctionDefinitionIndex,
        label: NopLabel,
        start_offset: CodeOffset,
    ) -> Result<()> {
        let func_entry = self
            .function_map
            .get_mut(&fdef_idx.0)
            .ok_or_else(|| format_err!("Tried to add nop mapping to undefined function index"))?;
        func_entry.add_nop_mapping(label, start_offset);
        Ok(())
    }

    /// Given a function definition and a code offset within that function definition, this returns
    /// the location in the source code associated with the instruction at that offset.
    pub fn get_code_location(
        &self,
        fdef_idx: FunctionDefinitionIndex,
        offset: CodeOffset,
    ) -> Result<Loc> {
        self.function_map
            .get(&fdef_idx.0)
            .and_then(|function_source_map| function_source_map.get_code_location(offset))
            .ok_or_else(|| format_err!("Tried to get code location from undefined function index"))
    }

    pub fn add_local_mapping(
        &mut self,
        fdef_idx: FunctionDefinitionIndex,
        name: SourceName,
    ) -> Result<()> {
        let func_entry = self
            .function_map
            .get_mut(&fdef_idx.0)
            .ok_or_else(|| format_err!("Tried to add local mapping to undefined function index"))?;
        func_entry.add_local_mapping(name);
        Ok(())
    }

    pub fn add_parameter_mapping(
        &mut self,
        fdef_idx: FunctionDefinitionIndex,
        name: SourceName,
    ) -> Result<()> {
        let func_entry = self.function_map.get_mut(&fdef_idx.0).ok_or_else(|| {
            format_err!("Tried to add parameter mapping to undefined function index")
        })?;
        func_entry.add_parameter_mapping(name);
        Ok(())
    }

    pub fn get_parameter_or_local_name(
        &self,
        fdef_idx: FunctionDefinitionIndex,
        index: u64,
    ) -> Result<SourceName> {
        self.function_map
            .get(&fdef_idx.0)
            .and_then(|function_source_map| function_source_map.get_parameter_or_local_name(index))
            .ok_or_else(|| format_err!("Tried to get local name at undefined function index"))
    }

    pub fn add_top_level_struct_mapping(
        &mut self,
        struct_def_idx: StructDefinitionIndex,
        location: Loc,
    ) -> Result<()> {
        self.struct_map.insert(struct_def_idx.0, StructSourceMap::new(location)).map_or(Ok(()), |_| { Err(format_err!(
                "Multiple structs at same struct definition index encountered when constructing source map"
                )) })
    }

    pub fn add_const_mapping(
        &mut self,
        const_idx: ConstantPoolIndex,
        name: ConstantName,
    ) -> Result<()> {
        self.constant_map
            .insert(name, const_idx.0)
            .map_or(Ok(()), |_| {
                Err(format_err!(
                    "Multiple constans with same name encountered when constructing source map"
                ))
            })
    }

    pub fn add_struct_field_mapping(
        &mut self,
        struct_def_idx: StructDefinitionIndex,
        location: Loc,
    ) -> Result<()> {
        let struct_entry = self
            .struct_map
            .get_mut(&struct_def_idx.0)
            .ok_or_else(|| format_err!("Tried to add file mapping to undefined struct index"))?;
        struct_entry.add_field_location(location);
        Ok(())
    }

    pub fn get_struct_field_name(
        &self,
        struct_def_idx: StructDefinitionIndex,
        field_idx: MemberCount,
    ) -> Option<Loc> {
        self.struct_map
            .get(&struct_def_idx.0)
            .and_then(|struct_source_map| struct_source_map.get_field_location(field_idx))
    }

    pub fn add_struct_type_parameter_mapping(
        &mut self,
        struct_def_idx: StructDefinitionIndex,
        name: SourceName,
    ) -> Result<()> {
        let struct_entry = self.struct_map.get_mut(&struct_def_idx.0).ok_or_else(|| {
            format_err!("Tried to add struct type parameter mapping to undefined struct index")
        })?;
        struct_entry.add_type_parameter(name);
        Ok(())
    }

    pub fn get_struct_type_parameter_name(
        &self,
        struct_def_idx: StructDefinitionIndex,
        type_parameter_idx: usize,
    ) -> Result<SourceName> {
        self.struct_map
            .get(&struct_def_idx.0)
            .and_then(|struct_source_map| {
                struct_source_map.get_type_parameter_name(type_parameter_idx)
            })
            .ok_or_else(|| format_err!("Unable to get struct type parameter name"))
    }

    pub fn get_function_source_map(
        &self,
        fdef_idx: FunctionDefinitionIndex,
    ) -> Result<&FunctionSourceMap> {
        self.function_map
            .get(&fdef_idx.0)
            .ok_or_else(|| format_err!("Unable to get function source map"))
    }

    pub fn get_struct_source_map(
        &self,
        struct_def_idx: StructDefinitionIndex,
    ) -> Result<&StructSourceMap> {
        self.struct_map
            .get(&struct_def_idx.0)
            .ok_or_else(|| format_err!("Unable to get struct source map"))
    }

    /// Create a 'dummy' source map for a compiled module or script. This is useful for e.g. disassembling
    /// with generated or real names depending upon if the source map is available or not.
    pub fn dummy_from_view(view: &BinaryIndexedView, default_loc: Loc) -> Result<Self> {
        let module_ident = match view {
            BinaryIndexedView::Script(..) => None,
            BinaryIndexedView::Module(..) => {
                let module_handle = view.module_handle_at(ModuleHandleIndex::new(0));
                let module_name = ModuleName(Symbol::from(
                    view.identifier_at(module_handle.name).as_str(),
                ));
                let address = *view.address_identifier_at(module_handle.address);
                Some(ModuleIdent::new(module_name, address))
            }
        };
        let mut empty_source_map = Self::new(default_loc, module_ident);

        match view {
            BinaryIndexedView::Script(script) => {
                empty_source_map.add_top_level_function_mapping(
                    FunctionDefinitionIndex(0_u16),
                    default_loc,
                    false,
                )?;
                empty_source_map
                    .function_map
                    .get_mut(&(0_u16))
                    .ok_or_else(|| {
                        format_err!("Unable to get function map while generating dummy")
                    })?
                    .dummy_function_map(
                        view,
                        &script.type_parameters,
                        script.parameters,
                        Some(script.code.clone()),
                        default_loc,
                    )?;
            }
            BinaryIndexedView::Module(module) => {
                for (function_idx, function_def) in module.function_defs.iter().enumerate() {
                    empty_source_map.add_top_level_function_mapping(
                        FunctionDefinitionIndex(function_idx as TableIndex),
                        default_loc,
                        false,
                    )?;
                    let function_handle = module.function_handle_at(function_def.function);
                    empty_source_map
                        .function_map
                        .get_mut(&(function_idx as TableIndex))
                        .ok_or_else(|| {
                            format_err!("Unable to get function map while generating dummy")
                        })?
                        .dummy_function_map(
                            view,
                            &function_handle.type_parameters,
                            function_handle.parameters,
                            function_def.code.clone(),
                            default_loc,
                        )?;
                }
            }
        };

        for (struct_idx, struct_def) in view.struct_defs().into_iter().flatten().enumerate() {
            empty_source_map.add_top_level_struct_mapping(
                StructDefinitionIndex(struct_idx as TableIndex),
                default_loc,
            )?;
            empty_source_map
                .struct_map
                .get_mut(&(struct_idx as TableIndex))
                .ok_or_else(|| format_err!("Unable to get struct map while generating dummy"))?
                .dummy_struct_map(view, struct_def, default_loc)?;
        }

        for const_idx in 0..view.constant_pool().len() {
            empty_source_map.add_const_mapping(
                ConstantPoolIndex(const_idx as TableIndex),
                ConstantName(Symbol::from(format!("CONST{}", const_idx))),
            )?;
        }

        Ok(empty_source_map)
    }
}
