// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::Parser;
use inline_colorization as IC;
use move_abstract_interpreter::control_flow_graph::{ControlFlowGraph, VMControlFlowGraph};
use move_binary_format::{
    file_format::{
        Ability, AbilitySet, Bytecode, CodeOffset, CodeUnit, Constant, ConstantPoolIndex,
        DatatypeTyParameter, EnumDefinitionIndex, FieldHandleIndex, FunctionDefinitionIndex,
        FunctionHandle, JumpTableInner, ModuleHandle, Signature, SignatureIndex, SignatureToken,
        StructDefinitionIndex, StructFieldInformation, TableIndex, TypeSignature, Visibility,
    },
    CompiledModule,
};
use move_bytecode_source_map::{
    mapping::SourceMapping,
    source_map::{FunctionSourceMap, SourceMap, SourceName},
};
use move_command_line_common::{
    display::{try_render_constant, RenderResult},
    files::FileHash,
};
use move_compiler::compiled_unit::CompiledUnit;
use move_core_types::{identifier::IdentStr, language_storage::ModuleId};
use move_coverage::coverage_map::{ExecCoverageMap, FunctionCoverage};
use move_ir_types::{
    ast::{ConstantName, ModuleIdent, ModuleName},
    location::Loc,
};
use move_symbol_pool::Symbol;
use std::{
    collections::{BTreeMap, HashMap},
    fmt::{self, Write},
    vec,
};

const PREVIEW_LEN: usize = 4;
const MAX_OUTPUT_SIZE: usize = 1024 * 1024;

/// Holds the various options that we support while disassembling code.
#[derive(Debug, Default, Parser)]
pub struct DisassemblerOptions {
    /// Only print non-private functions
    #[clap(long = "only-public")]
    pub only_externally_visible: bool,

    /// Print the bytecode for the instructions within the function.
    #[clap(long = "print-code")]
    pub print_code: bool,

    /// Print the basic blocks of the bytecode.
    #[clap(long = "print-basic-blocks")]
    pub print_basic_blocks: bool,

    /// Print the locals inside each function body.
    #[clap(long = "print-locals")]
    pub print_locals: bool,

    /// Maximum size of the output. If the output exceeds this size, the disassembler will return
    /// an error.
    #[clap(long = "max-output-size")]
    pub max_output_size: Option<usize>,
}

impl DisassemblerOptions {
    pub fn new() -> Self {
        Self {
            only_externally_visible: false,
            print_code: true,
            print_basic_blocks: true,
            print_locals: true,
            max_output_size: Some(MAX_OUTPUT_SIZE),
        }
    }
}

pub struct Disassembler<'a> {
    source_mapper: SourceMapping<'a>,
    // The various options that we can set for disassembly.
    options: DisassemblerOptions,
    // Optional coverage map for use in displaying code coverage
    coverage_map: Option<ExecCoverageMap>,
    /// If the code being disassembled imports multiple modules of the form (a, SameModuleName)
    /// `module_alias` will contain an entry for each distinct a
    /// e.g., for `use 0xA::M; use 0xB::M`, this will contain [(0xA, M) -> M, (0xB, M) -> 1M]
    module_aliases: HashMap<ModuleId, String>,
}

struct BoundedBuffer<'a> {
    budget: usize,
    buf: &'a mut String,
}

impl<'a> Write for BoundedBuffer<'a> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if self.budget < s.len() {
            return Err(fmt::Error);
        }
        self.budget -= s.len();
        self.buf.push_str(s);
        Ok(())
    }
}

trait ByteLength {
    fn byte_len(&self) -> u32;
}

impl<'a> ByteLength for BoundedBuffer<'a> {
    fn byte_len(&self) -> u32 {
        self.buf.len() as u32
    }
}

impl ByteLength for String {
    fn byte_len(&self) -> u32 {
        self.len() as u32
    }
}

macro_rules! any_writeln {
    ($buf:expr) => {
        any_writeln!($buf,)
    };
    ($buf:expr, $($args:tt)*) => {{
        let start_offset = $buf.byte_len();
        let res = std::writeln!($buf, $($args)*).map_err(anyhow::Error::from);
        let end_offset= $buf.byte_len();
        res.map(|_| Loc::new(FileHash::empty(), start_offset, end_offset))
    }};
}

macro_rules! any_write {
    ($buf:expr) => {
        any_write!($buf,)
    };
    ($buf:expr, $($args:tt)*) => {{
        let start_offset = $buf.byte_len();
        let res = std::write!($buf, $($args)*).map_err(anyhow::Error::from);
        let end_offset= $buf.byte_len();
        res.map(|_| Loc::new(FileHash::empty(), start_offset, end_offset))
    }};
}

fn delimited_list<T, F, W>(
    items: impl IntoIterator<Item = T>,
    prefix: &str,
    delimiter: &str,
    suffix: &str,
    buf: &mut W,
    mut printer: F,
) -> Result<()>
where
    W: Write + ByteLength,
    F: FnMut(&mut W, T) -> Result<()>,
{
    let mut first = prefix;
    let mut last = "";
    for item in items {
        buf.write_str(first)?;
        first = delimiter;
        last = suffix;
        printer(buf, item)?;
    }
    buf.write_str(last)?;

    Ok(())
}

impl<'a> Disassembler<'a> {
    pub fn new(source_mapper: SourceMapping<'a>, options: DisassemblerOptions) -> Self {
        let mut module_names = HashMap::new();
        let mut module_aliases = HashMap::new();
        module_names.insert(source_mapper.bytecode.self_id().name().to_string(), 0);
        for h in source_mapper.bytecode.module_handles() {
            let id = source_mapper.bytecode.module_id_for_handle(h);
            let module_name = id.name().to_string();
            module_names
                .entry(module_name.clone())
                .and_modify(|name_count| {
                    // This module imports >1 modules named `name`--add alias <count><module_name> for `id`.
                    // Move identifiers cannot begin with an integer,
                    // so this is guaranteed not to conflict with other module names.
                    module_aliases.insert(id, format!("{}{}", name_count, module_name));
                    *name_count += 1;
                })
                .or_insert(0);
        }
        Self {
            source_mapper,
            options,
            coverage_map: None,
            module_aliases,
        }
    }

    pub fn from_module(module: &'a CompiledModule, default_loc: Loc) -> Result<Self> {
        Self::from_module_with_max_size(module, default_loc, None)
    }

    pub fn from_module_with_max_size(
        module: &'a CompiledModule,
        default_loc: Loc,
        max_size: Option<usize>,
    ) -> Result<Self> {
        let mut options = DisassemblerOptions::new();
        options.print_code = true;
        options.max_output_size = max_size;
        Ok(Self::new(
            SourceMapping::new_without_source_map(module, default_loc)?,
            options,
        ))
    }

    pub fn from_unit(unit: &'a CompiledUnit) -> Self {
        let options = DisassemblerOptions::new();
        let source_map = unit.source_map().clone();

        let source_mapping = SourceMapping::new(source_map, &unit.module);
        Disassembler::new(source_mapping, options)
    }

    pub fn add_coverage_map(&mut self, coverage_map: ExecCoverageMap) {
        self.coverage_map = Some(coverage_map);
    }

    /// Disassemble the module and return the disassembled string.
    pub fn disassemble(&self) -> Result<String> {
        let mut buffer = String::new();
        let bcode_map_gen = false;
        if let Some(budget) = self.options.max_output_size {
            self.print_module(
                &mut BoundedBuffer {
                    buf: &mut buffer,
                    budget,
                },
                bcode_map_gen,
            )
            .map_err(|e| anyhow::anyhow!("{e}: Module exceeded max allowed disassembly size"))?;
        } else {
            self.print_module(&mut buffer, bcode_map_gen)?;
        };
        Ok(buffer)
    }

    /// Disassemble the module and return the disassembled string,
    /// but also a source map for the disassembled bytecode.
    pub fn disassemble_with_source_map(&self) -> Result<(String, SourceMap)> {
        let mut buffer = String::new();
        let bcode_map_gen = true;
        let mut bcode_map = if let Some(budget) = self.options.max_output_size {
            self.print_module(
                &mut BoundedBuffer {
                    buf: &mut buffer,
                    budget,
                },
                bcode_map_gen,
            )
            .map_err(|e| anyhow::anyhow!("{e}: Module exceeded max allowed disassembly size"))?
        } else {
            self.print_module(&mut buffer, bcode_map_gen)?
        };
        let file_hash = FileHash::new(&buffer);
        bcode_map.replace_file_hashes(file_hash);
        Ok((buffer, bcode_map))
    }
}

// Note on naming:
// * disassemble_* and print_* functions are functions that output to the buffer
// * format_* functions return a string that can be used in the buffer
impl<'a> Disassembler<'a> {
    fn print_module(
        &self,
        buffer: &mut (impl Write + ByteLength),
        bcode_map_gen: bool,
    ) -> Result<SourceMap> {
        // NB: The order in which these are called is important as each function is effectful.
        let mut res = Some(self.print_header(buffer)?);
        let bcode_map_opt = if bcode_map_gen { &mut res } else { &mut None };
        self.print_imports(buffer)?;
        self.print_user_defined_types(buffer, bcode_map_opt)?;
        self.print_function_definitions(buffer, bcode_map_opt)?;
        self.print_constants(buffer, bcode_map_opt)?;
        self.print_footer(buffer)?;
        Ok(res.unwrap())
    }

    fn print_header(&self, buffer: &mut (impl Write + ByteLength)) -> Result<SourceMap> {
        let (addr, n) = &self.source_mapper.source_map.module_name;
        any_write!(
            buffer,
            "// Move bytecode v{version}\nmodule {addr}.",
            version = self.source_mapper.bytecode.version(),
            addr = addr.short_str_lossless(),
        )?;
        let mod_name_loc = any_write!(buffer, "{name}", name = n)?;
        any_writeln!(buffer, " {{")?;
        let mod_ident = ModuleIdent::new(ModuleName(Symbol::from(n.as_str())), *addr);
        let bcode_map = SourceMap::new(mod_name_loc, mod_ident);
        Ok(bcode_map)
    }

    fn print_imports(&self, buffer: &mut (impl Write + ByteLength)) -> Result<()> {
        for h in self.source_mapper.bytecode.module_handles().iter() {
            self.disassemble_import(buffer, h)?;
        }

        if !self.source_mapper.bytecode.module_handles().is_empty() {
            any_writeln!(buffer)?;
        }

        Ok(())
    }

    fn print_user_defined_types(
        &self,
        buffer: &mut (impl Write + ByteLength),
        bcode_map_opt: &mut Option<SourceMap>,
    ) -> Result<()> {
        for i in 0..self.source_mapper.bytecode.struct_defs().len() {
            self.disassemble_struct_def(
                buffer,
                bcode_map_opt,
                StructDefinitionIndex(i as TableIndex),
            )?;
            any_writeln!(buffer)?;
        }

        for i in 0..self.source_mapper.bytecode.enum_defs().len() {
            self.disassemble_enum_def(buffer, bcode_map_opt, EnumDefinitionIndex(i as TableIndex))?;
            any_writeln!(buffer)?;
        }

        Ok(())
    }

    fn print_function_definitions(
        &self,
        buffer: &mut (impl Write + ByteLength),
        bcode_map_opt: &mut Option<SourceMap>,
    ) -> Result<()> {
        for i in 0..self.source_mapper.bytecode.function_defs().len() {
            self.disassemble_function_definition(
                buffer,
                bcode_map_opt,
                FunctionDefinitionIndex(i as TableIndex),
            )?;
            any_writeln!(buffer)?;
        }

        Ok(())
    }

    fn print_constants(
        &self,
        buffer: &mut (impl Write + ByteLength),
        bcode_map_opt: &mut Option<SourceMap>,
    ) -> Result<()> {
        delimited_list(
            self.source_mapper
                .bytecode
                .constant_pool()
                .iter()
                .enumerate(),
            "Constants [\n",
            "",
            "]\n",
            buffer,
            |buffer, (idx, constant)| {
                if let Some(bcode_map) = bcode_map_opt {
                    // no constant name in the disassembled bytecode - use index as a name
                    bcode_map.add_const_mapping(
                        ConstantPoolIndex(idx as TableIndex),
                        ConstantName(Symbol::from(idx.to_string())),
                    )?;
                }
                self.disassemble_constant(buffer, idx, constant, false)
            },
        )
    }

    fn print_footer(&self, buffer: &mut (impl Write + ByteLength)) -> Result<()> {
        any_writeln!(buffer, "}}").map(|_| ())
    }

    //***************************************************************************
    // Disassemblers (that output directly to the buffer)
    //***************************************************************************

    // The struct defs will filter out the structs that we print to only be the ones that are
    // defined in the module in question.
    fn disassemble_struct_def(
        &self,
        buffer: &mut (impl Write + ByteLength),
        bcode_map_opt: &mut Option<SourceMap>,
        struct_def_idx: StructDefinitionIndex,
    ) -> Result<()> {
        let struct_definition = self.source_mapper.bytecode.struct_def_at(struct_def_idx);
        let struct_handle = self
            .source_mapper
            .bytecode
            .datatype_handle_at(struct_definition.struct_handle);
        let struct_source_map = self
            .source_mapper
            .source_map
            .get_struct_source_map(struct_def_idx)?;

        let field_info: Option<Vec<(&IdentStr, &TypeSignature)>> =
            match &struct_definition.field_information {
                StructFieldInformation::Native => None,
                StructFieldInformation::Declared(fields) => Some(
                    fields
                        .iter()
                        .map(|field_definition| {
                            let type_sig = &field_definition.signature;
                            let field_name = self
                                .source_mapper
                                .bytecode
                                .identifier_at(field_definition.name);
                            (field_name, type_sig)
                        })
                        .collect(),
                ),
            };

        let native = if field_info.is_none() { "native " } else { "" };
        let name = self
            .source_mapper
            .bytecode
            .identifier_at(struct_handle.name)
            .to_string();

        any_write!(buffer, "{native}struct ")?;
        let struct_name_loc = any_write!(buffer, "{name}")?;
        if let Some(bcode_map) = bcode_map_opt {
            bcode_map.add_top_level_struct_mapping(struct_def_idx, struct_name_loc)?;
        }

        let type_param_source_names = Self::disassemble_datatype_type_formals(
            buffer,
            &struct_source_map.type_parameters,
            &struct_handle.type_parameters,
        )?;
        if let Some(bcode_map) = bcode_map_opt {
            for n in type_param_source_names {
                bcode_map.add_struct_type_parameter_mapping(struct_def_idx, n)?;
            }
        }

        Self::disassemble_abilites(buffer, struct_handle.abilities)?;

        any_write!(buffer, " {{")?;

        match field_info {
            None => (),
            Some(field_info) => {
                delimited_list(
                    &field_info,
                    "\n",
                    ",\n",
                    "",
                    buffer,
                    |buffer, (name, ty)| {
                        any_write!(buffer, "\t")?;
                        let field_name_loc = any_write!(buffer, "{name}")?;
                        if let Some(bcode_map) = bcode_map_opt {
                            bcode_map.add_struct_field_mapping(struct_def_idx, field_name_loc)?;
                        }
                        any_write!(buffer, ": ")?;

                        self.disassemble_sig_tok(
                            buffer,
                            &ty.0,
                            None,
                            &struct_source_map.type_parameters,
                        )
                    },
                )?;
                any_writeln!(buffer)?;
            }
        }

        any_writeln!(buffer, "}}")?;

        Ok(())
    }

    fn disassemble_enum_def(
        &self,
        buffer: &mut (impl Write + ByteLength),
        bcode_map_opt: &mut Option<SourceMap>,
        enum_def_idx: EnumDefinitionIndex,
    ) -> Result<()> {
        let enum_definition = self.source_mapper.bytecode.enum_def_at(enum_def_idx);
        let enum_handle = self
            .source_mapper
            .bytecode
            .datatype_handle_at(enum_definition.enum_handle);
        let enum_source_map = self
            .source_mapper
            .source_map
            .get_enum_source_map(enum_def_idx)?;

        let name = self
            .source_mapper
            .bytecode
            .identifier_at(enum_handle.name)
            .to_string();

        any_write!(buffer, "enum ")?;
        let enum_name_loc = any_write!(buffer, "{name}")?;
        if let Some(bcode_map) = bcode_map_opt {
            bcode_map.add_top_level_enum_mapping(enum_def_idx, enum_name_loc)?;
        }

        let type_param_source_names = Self::disassemble_datatype_type_formals(
            buffer,
            &enum_source_map.type_parameters,
            &enum_handle.type_parameters,
        )?;
        if let Some(bcode_map) = bcode_map_opt {
            for n in type_param_source_names {
                bcode_map.add_enum_type_parameter_mapping(enum_def_idx, n)?;
            }
        }

        Self::disassemble_abilites(buffer, enum_handle.abilities)?;

        delimited_list(
            &enum_definition.variants,
            " {",
            ",",
            "\n}\n",
            buffer,
            |buffer, variant| {
                let variant_name = self
                    .source_mapper
                    .bytecode
                    .identifier_at(variant.variant_name);

                any_write!(buffer, "\n\t")?;
                let variant_start_offset = buffer.byte_len();
                any_write!(buffer, "{variant_name} {{ ")?;

                let mut field_locs = vec![];
                delimited_list(
                    &variant.fields,
                    "",
                    ", ",
                    "",
                    buffer,
                    |buffer, field_definition| {
                        let type_sig = &field_definition.signature;
                        let field_name = self
                            .source_mapper
                            .bytecode
                            .identifier_at(field_definition.name);
                        let field_name_loc = any_write!(buffer, "{field_name}")?;
                        field_locs.push(field_name_loc);

                        any_write!(buffer, ": ")?;
                        self.disassemble_sig_tok(
                            buffer,
                            &type_sig.0,
                            None,
                            &enum_source_map.type_parameters,
                        )
                    },
                )?;

                any_write!(buffer, " }}")?;
                // perhaps surprisingly, but the location is of the whole variant
                // and not just of its name
                let variant_end_offset = buffer.byte_len();
                if let Some(bcode_map) = bcode_map_opt {
                    let variant_loc =
                        Loc::new(FileHash::empty(), variant_start_offset, variant_end_offset);
                    bcode_map.add_enum_variant_mapping(
                        enum_def_idx,
                        (variant_name.to_string(), variant_loc),
                        field_locs,
                    )?;
                }
                Ok(())
            },
        )?;

        Ok(())
    }

    fn add_function_bytecode_map(
        &self,
        bcode_map: &mut SourceMap,
        function_definition_index: FunctionDefinitionIndex,
        fun_name_loc: Loc,
        fun_def_loc: Loc,
        is_native: bool,
        type_param_source_names: Vec<SourceName>,
        param_source_names: Vec<SourceName>,
        return_locs: Vec<Loc>,
        locals: Vec<SourceName>,
        code_locations: BTreeMap<CodeOffset, Loc>,
    ) -> Result<()> {
        bcode_map.add_top_level_function_mapping(
            function_definition_index,
            fun_name_loc,
            fun_def_loc,
            is_native,
        )?;
        for n in type_param_source_names {
            bcode_map.add_function_type_parameter_mapping(function_definition_index, n)?;
        }
        for n in param_source_names {
            bcode_map.add_parameter_mapping(function_definition_index, n)?;
        }
        for loc in return_locs {
            bcode_map.add_return_mapping(function_definition_index, loc)?;
        }
        for name in locals {
            bcode_map.add_local_mapping(function_definition_index, name)?;
        }
        for (offset, loc) in code_locations {
            bcode_map.add_code_mapping(function_definition_index, offset, loc)?;
        }
        Ok(())
    }

    fn disassemble_function_definition(
        &self,
        buffer: &mut (impl Write + ByteLength),
        bcode_map_opt: &mut Option<SourceMap>,
        function_definition_index: FunctionDefinitionIndex,
    ) -> Result<()> {
        let function_definition = self
            .source_mapper
            .bytecode
            .function_def_at(function_definition_index);
        let function_handle = self
            .source_mapper
            .bytecode
            .function_handle_at(function_definition.function);
        let function_source_map = self
            .source_mapper
            .source_map
            .get_function_source_map(function_definition_index)?;
        let parameters = function_handle.parameters;
        let name = self
            .source_mapper
            .bytecode
            .identifier_at(function_handle.name)
            .to_owned();
        let function = self
            .source_mapper
            .bytecode
            .function_def_at(function_definition_index);
        debug_assert_eq!(
            function_source_map.parameters.len(),
            self.source_mapper.bytecode.signature_at(parameters).len(),
            "Arity mismatch between function source map and bytecode for function {name}",
        );

        let visibility_modifier = match function.visibility {
            Visibility::Private => {
                if self.options.only_externally_visible {
                    return Ok(());
                } else {
                    ""
                }
            }
            Visibility::Friend => "public(friend) ",
            Visibility::Public => "public ",
        };

        let fun_def_start_offset = buffer.byte_len();
        let entry_modifier = if function.is_entry { "entry " } else { "" };
        let native_modifier = if function.is_native() { "native " } else { "" };

        any_write!(
            buffer,
            "{entry_modifier}{native_modifier}{visibility_modifier}",
        )?;

        let fun_name_loc = any_write!(buffer, "{name}",)?;

        let type_param_source_names = Self::disassemble_fun_type_formals(
            buffer,
            &function_source_map.type_parameters,
            &function_handle.type_parameters,
        )?;

        any_write!(buffer, "(")?;

        let mut param_source_names = vec![];
        delimited_list(
            self.source_mapper
                .bytecode
                .signature_at(parameters)
                .0
                .iter()
                .zip(function_source_map.parameters.iter()),
            "",
            ", ",
            "",
            buffer,
            |buffer, (tok, (name, _))| {
                let name_loc = any_write!(buffer, "{name}")?;
                param_source_names.push((name.to_string(), name_loc));
                any_write!(buffer, ": ")?;
                self.disassemble_sig_tok(buffer, tok, None, &function_source_map.type_parameters)
            },
        )?;

        any_write!(buffer, ")")?;

        let mut return_locs = vec![];
        delimited_list(
            &self
                .source_mapper
                .bytecode
                .signature_at(function_handle.return_)
                .0,
            ": ",
            " * ",
            "",
            buffer,
            |buffer, tok| {
                let return_type_start_offset = buffer.byte_len();
                self.disassemble_sig_tok(buffer, tok, None, &function_source_map.type_parameters)?;
                let return_type_end_offset = buffer.byte_len();
                let return_type_loc = Loc::new(
                    FileHash::empty(),
                    return_type_start_offset,
                    return_type_end_offset,
                );
                return_locs.push(return_type_loc);
                Ok(())
            },
        )?;

        let Some(code) = &function.code else {
            any_writeln!(buffer, ";")?;
            let fun_def_end_offset = buffer.byte_len();
            if let Some(bcode_map) = bcode_map_opt {
                let fun_def_loc =
                    Loc::new(FileHash::empty(), fun_def_start_offset, fun_def_end_offset);
                self.add_function_bytecode_map(
                    bcode_map,
                    function_definition_index,
                    fun_name_loc,
                    fun_def_loc,
                    function.is_native(),
                    type_param_source_names,
                    param_source_names,
                    return_locs,
                    vec![],
                    BTreeMap::new(),
                )?;
            }
            return Ok(());
        };

        let params_len = self.source_mapper.bytecode.signature_at(parameters).0.len();

        any_writeln!(buffer, " {{")?;
        let locals =
            self.disassemble_locals(buffer, function_source_map, code.locals, params_len)?;
        let code_locations =
            self.disassemble_bytecode(buffer, function_source_map, &name, parameters, code)?;
        self.disassemble_jump_tables(buffer, code)?;
        any_writeln!(buffer, "}}")?;

        let fun_def_end_offset = buffer.byte_len();
        if let Some(bcode_map) = bcode_map_opt {
            let fun_loc = Loc::new(FileHash::empty(), fun_def_start_offset, fun_def_end_offset);
            self.add_function_bytecode_map(
                bcode_map,
                function_definition_index,
                fun_loc,
                fun_name_loc,
                function.is_native(),
                type_param_source_names,
                param_source_names,
                return_locs,
                locals,
                code_locations,
            )?;
        }
        Ok(())
    }

    fn disassemble_locals(
        &self,
        buffer: &mut (impl Write + ByteLength),
        function_source_map: &FunctionSourceMap,
        locals_idx: SignatureIndex,
        parameter_len: usize,
    ) -> Result<Vec<SourceName>> {
        let mut locals = vec![];
        if !self.options.print_locals {
            return Ok(locals);
        }

        let signature = self.source_mapper.bytecode.signature_at(locals_idx);
        for (local_idx, (name, _)) in function_source_map.locals.iter().enumerate() {
            any_write!(buffer, "L{}:\t", local_idx + parameter_len)?;
            let name_loc = any_write!(buffer, "{name}")?;
            any_write!(buffer, ": ")?;
            locals.push((name.clone(), name_loc));
            self.disassemble_type_for_local(buffer, function_source_map, local_idx, signature)?;
            any_writeln!(buffer)?;
        }

        Ok(locals)
    }

    fn disassemble_jump_tables(
        &self,
        buffer: &mut (impl Write + ByteLength),
        code: &CodeUnit,
    ) -> Result<()> {
        if !self.options.print_code || code.jump_tables.is_empty() {
            return Ok(());
        }

        any_writeln!(buffer, "Jump tables:")?;

        for (i, jt) in code.jump_tables.iter().enumerate() {
            let enum_def = self.source_mapper.bytecode.enum_def_at(jt.head_enum);
            let enum_handle = self
                .source_mapper
                .bytecode
                .datatype_handle_at(enum_def.enum_handle);
            let enum_source_map = self
                .source_mapper
                .source_map
                .get_enum_source_map(jt.head_enum)?;
            let enum_name = self.source_mapper.bytecode.identifier_at(enum_handle.name);
            let JumpTableInner::Full(jt) = &jt.jump_table;
            any_writeln!(buffer, "[{i}]:\tvariant_switch {enum_name} {{")?;
            for (tag, jump_loc) in jt.iter().enumerate() {
                let enum_name = enum_source_map
                    .get_variant_location(tag as u16)
                    .map(|((name, _), _)| name)
                    .unwrap_or(format!("Variant{}", tag));
                any_writeln!(buffer, "\t\t{enum_name} => jump {jump_loc}")?;
            }
            any_writeln!(buffer, "\t}}")?;
        }
        Ok(())
    }

    fn disassemble_bytecode(
        &self,
        buffer: &mut (impl Write + ByteLength),
        function_source_map: &FunctionSourceMap,
        function_name: &IdentStr,
        parameters: SignatureIndex,
        code: &CodeUnit,
    ) -> Result<BTreeMap<CodeOffset, Loc>> {
        let mut code_offsets = BTreeMap::new();
        if !self.options.print_code {
            return Ok(code_offsets);
        }

        let parameters = self.source_mapper.bytecode.signature_at(parameters);
        let locals_sigs = self.source_mapper.bytecode.signature_at(code.locals);
        let function_code_coverage_map = self.get_function_coverage(function_name).cloned();
        let cfg_opt = if self.options.print_basic_blocks {
            let cfg: BTreeMap<_, _> = VMControlFlowGraph::new(&code.code, &code.jump_tables)
                .blocks()
                .into_iter()
                .enumerate()
                .map(|(block_number, pc_start)| (pc_start, block_number))
                .collect();
            Some(cfg)
        } else {
            None
        };

        let coverage_enabled = self.coverage_map.is_some();

        for (pc, instruction) in code.code.iter().enumerate() {
            if let Some(block_number) = cfg_opt.as_ref().and_then(|cfg| cfg.get(&(pc as u16))) {
                any_writeln!(buffer, "B{block_number}:")?;
            }

            match &function_code_coverage_map {
                None => {
                    any_write!(buffer, "\t{pc}: ")?;
                }
                Some(coverage_map) => {
                    let coverage = coverage_map.get(&(pc as u64));
                    match coverage {
                        Some(coverage) => {
                            any_write!(buffer, "{}[{coverage}]\t{pc}: ", IC::color_green)?;
                        }
                        None => {
                            any_write!(buffer, "{}\t{pc}: ", IC::color_red)?;
                        }
                    }
                }
            }

            let inst_loc = self.disassemble_instruction(
                buffer,
                function_source_map,
                parameters,
                locals_sigs,
                instruction,
            )?;

            code_offsets.insert(pc as CodeOffset, inst_loc);

            any_writeln!(
                buffer,
                "{}",
                if coverage_enabled {
                    IC::color_reset
                } else {
                    ""
                }
            )?;
        }

        Ok(code_offsets)
    }

    fn disassemble_import(
        &self,
        buffer: &mut (impl Write + ByteLength),
        module_handle: &ModuleHandle,
    ) -> Result<()> {
        let module_id = self
            .source_mapper
            .bytecode
            .module_id_for_handle(module_handle);
        if self.is_self_id(&module_id) {
            // No need to import self handle
            Ok(())
        } else {
            if let Some(alias) = self.module_aliases.get(&module_id) {
                any_writeln!(
                    buffer,
                    "use {}::{} as {};",
                    module_id.address(),
                    module_id.name(),
                    alias
                )
            } else {
                any_writeln!(buffer, "use {}::{};", module_id.address(), module_id.name())
            }
            .map(|_| ())
        }
    }

    fn disassemble_instruction(
        &self,
        buffer: &mut (impl Write + ByteLength),
        function_source_map: &FunctionSourceMap,
        parameters: &Signature,
        locals_sigs: &Signature,
        instruction: &Bytecode,
    ) -> Result<Loc> {
        macro_rules! parens {
            ($($args:tt)*) => {{
                any_write!(buffer, "(")?;
                $($args)*
                any_write!(buffer, ")").map(|_| ())
            }};
        }

        let inst_start_offset = buffer.byte_len();
        match instruction {
            Bytecode::LdConst(idx) => {
                any_write!(buffer, "LdConst[{idx}]")?;
                parens! {
                    let constant = self.source_mapper.bytecode.constant_at(*idx);
                    self.disassemble_constant(buffer, idx.0 as usize, constant, true)?;
                }
            }
            Bytecode::CopyLoc(local_idx) => {
                any_write!(buffer, "CopyLoc[{local_idx}]")?;
                parens! {
                    let name = self.format_name_for_parameter_or_local(
                        function_source_map,
                        usize::from(*local_idx),
                    );
                    any_write!(buffer, "{name}: ")?;
                    self.disassemble_type_for_parameter_or_local(
                        buffer,
                        function_source_map,
                        usize::from(*local_idx),
                        parameters,
                        locals_sigs,
                    )?;
                }
            }
            Bytecode::MoveLoc(local_idx) => {
                any_write!(buffer, "MoveLoc[{local_idx}]")?;
                parens! {
                    let name = self.format_name_for_parameter_or_local(
                        function_source_map,
                        usize::from(*local_idx),
                    );
                    any_write!(buffer, "{name}: ")?;
                    self.disassemble_type_for_parameter_or_local(
                        buffer,
                        function_source_map,
                        usize::from(*local_idx),
                        parameters,
                        locals_sigs,
                    )?;
                }
            }
            Bytecode::StLoc(local_idx) => {
                any_write!(buffer, "StLoc[{local_idx}]")?;
                parens! {
                    let name = self.format_name_for_parameter_or_local(
                        function_source_map,
                        usize::from(*local_idx),
                    );
                    any_write!(buffer, "{name}: ")?;
                    self.disassemble_type_for_parameter_or_local(
                        buffer,
                        function_source_map,
                        usize::from(*local_idx),
                        parameters,
                        locals_sigs,
                    )?;
                }
            }
            Bytecode::MutBorrowLoc(local_idx) => {
                any_write!(buffer, "MutBorrowLoc[{local_idx}]")?;
                parens! {
                    let name = self.format_name_for_parameter_or_local(
                        function_source_map,
                        usize::from(*local_idx),
                    );
                    any_write!(buffer, "{name}: ")?;
                    self.disassemble_type_for_parameter_or_local(
                        buffer,
                        function_source_map,
                        usize::from(*local_idx),
                        parameters,
                        locals_sigs,
                    )?;
                }
            }
            Bytecode::ImmBorrowLoc(local_idx) => {
                any_write!(buffer, "ImmBorrowLoc[{local_idx}]")?;
                parens! {
                    let name = self.format_name_for_parameter_or_local(
                        function_source_map,
                        usize::from(*local_idx),
                    );
                    any_write!(buffer, "{name}: ")?;
                    self.disassemble_type_for_parameter_or_local(
                        buffer,
                        function_source_map,
                        usize::from(*local_idx),
                        parameters,
                        locals_sigs,
                    )?;
                }
            }
            Bytecode::MutBorrowField(field_idx) => {
                any_write!(buffer, "MutBorrowField[{field_idx}]")?;
                parens! {
                    self.disassemble_struct_field_access(buffer, *field_idx)?;
                    any_write!(buffer, ": ")?;
                    self.disassemble_type_for_field(buffer, function_source_map, None, *field_idx)?;
                }
            }
            Bytecode::MutBorrowFieldGeneric(field_idx) => {
                any_write!(buffer, "MutBorrowFieldGeneric[{field_idx}]")?;
                parens! {
                    let field_inst = self
                        .source_mapper
                        .bytecode
                        .field_instantiation_at(*field_idx);
                    self.disassemble_struct_field_access(buffer, field_inst.handle)?;
                    any_write!(buffer, ": ")?;
                    let instantiation = self
                        .source_mapper
                        .bytecode
                        .signature_at(field_inst.type_parameters);
                    self.disassemble_type_for_field(buffer, function_source_map, Some(&instantiation.0), field_inst.handle)?;
                }
            }
            Bytecode::ImmBorrowField(field_idx) => {
                any_write!(buffer, "ImmBorrowField[{field_idx}]")?;
                parens! {
                    self.disassemble_struct_field_access(buffer, *field_idx)?;
                    any_write!(buffer, ": ")?;
                    self.disassemble_type_for_field(buffer, function_source_map, None, *field_idx)?;
                }
            }
            Bytecode::ImmBorrowFieldGeneric(field_idx) => {
                any_write!(buffer, "ImmBorrowFieldGeneric[{field_idx}]")?;
                parens! {
                    let field_inst = self
                        .source_mapper
                        .bytecode
                        .field_instantiation_at(*field_idx);
                    self.disassemble_struct_field_access(buffer, field_inst.handle)?;
                    any_write!(buffer, ": ")?;
                    let instantiation = self
                        .source_mapper
                        .bytecode
                        .signature_at(field_inst.type_parameters);
                    self.disassemble_type_for_field(buffer, function_source_map, Some(&instantiation.0), field_inst.handle)?;
                }
            }
            Bytecode::Pack(struct_idx) => {
                any_write!(buffer, "Pack[{struct_idx}]")?;
                parens! {
                    self.disassemble_struct_call(
                        buffer,
                        function_source_map,
                        *struct_idx,
                        &Signature(vec![]),
                    )?;
                }
            }
            Bytecode::PackGeneric(struct_idx) => {
                any_write!(buffer, "PackGeneric[{struct_idx}]")?;
                parens! {
                    let struct_inst = self
                        .source_mapper
                        .bytecode
                        .struct_instantiation_at(*struct_idx);
                    let type_params = self
                        .source_mapper
                        .bytecode
                        .signature_at(struct_inst.type_parameters);
                    self.disassemble_struct_call(
                        buffer,
                        function_source_map,
                        struct_inst.def,
                        type_params,
                    )?;
                }
            }
            Bytecode::Unpack(struct_idx) => {
                any_write!(buffer, "Unpack[{struct_idx}]")?;
                parens! {
                    self.disassemble_struct_call(
                        buffer,
                        function_source_map,
                        *struct_idx,
                        &Signature(vec![]),
                    )?;
                }
            }
            Bytecode::UnpackGeneric(struct_idx) => {
                any_write!(buffer, "UnpackGeneric[{struct_idx}]")?;
                parens! {
                    let struct_inst = self
                        .source_mapper
                        .bytecode
                        .struct_instantiation_at(*struct_idx);
                    let type_params = self
                        .source_mapper
                        .bytecode
                        .signature_at(struct_inst.type_parameters);
                    self.disassemble_struct_call(
                        buffer,
                        function_source_map,
                        struct_inst.def,
                        type_params,
                    )?;
                }
            }
            Bytecode::Call(method_idx) => {
                let function_handle = self.source_mapper.bytecode.function_handle_at(*method_idx);
                let module_handle = self
                    .source_mapper
                    .bytecode
                    .module_handle_at(function_handle.module);
                any_write!(buffer, "Call ")?;
                self.disassemble_function_string(buffer, module_handle, function_handle)?;
                parens! {
                    delimited_list(
                        self.source_mapper.bytecode.signature_at(function_handle.parameters).0.iter(),
                        "",
                        ", ",
                        "",
                        buffer,
                        |buffer, tok| {
                            self.disassemble_sig_tok(buffer, tok, None, &[])
                        },
                    )?;
                }?;
                delimited_list(
                    &self
                        .source_mapper
                        .bytecode
                        .signature_at(function_handle.return_)
                        .0,
                    ": ",
                    " * ",
                    "",
                    buffer,
                    |buffer, tok| self.disassemble_sig_tok(buffer, tok, None, &[]),
                )
            }
            Bytecode::CallGeneric(method_idx) => {
                let func_inst = self
                    .source_mapper
                    .bytecode
                    .function_instantiation_at(*method_idx);
                let function_handle = self
                    .source_mapper
                    .bytecode
                    .function_handle_at(func_inst.handle);
                let module_handle = self
                    .source_mapper
                    .bytecode
                    .module_handle_at(function_handle.module);
                any_write!(buffer, "Call ")?;
                self.disassemble_function_string(buffer, module_handle, function_handle)?;
                let func_instantiation = &self
                    .source_mapper
                    .bytecode
                    .signature_at(func_inst.type_parameters)
                    .0;
                delimited_list(func_instantiation, "<", ", ", ">", buffer, |buffer, ty| {
                    self.disassemble_sig_tok(buffer, ty, None, &function_source_map.type_parameters)
                })?;
                parens! {
                    delimited_list(
                        self.source_mapper.bytecode.signature_at(function_handle.parameters).0.iter(),
                        "",
                        ", ",
                        "",
                        buffer,
                        |buffer, tok| {
                            self.disassemble_sig_tok(buffer, tok, Some(func_instantiation), &function_source_map.type_parameters)
                        },
                    )?;
                }?;
                delimited_list(
                    &self
                        .source_mapper
                        .bytecode
                        .signature_at(function_handle.return_)
                        .0,
                    ": ",
                    " * ",
                    "",
                    buffer,
                    |buffer, tok| {
                        self.disassemble_sig_tok(
                            buffer,
                            tok,
                            Some(func_instantiation),
                            &function_source_map.type_parameters,
                        )
                    },
                )
            }
            Bytecode::ExistsDeprecated(_)
            | Bytecode::ExistsGenericDeprecated(_)
            | Bytecode::MutBorrowGlobalDeprecated(_)
            | Bytecode::MutBorrowGlobalGenericDeprecated(_)
            | Bytecode::ImmBorrowGlobalDeprecated(_)
            | Bytecode::ImmBorrowGlobalGenericDeprecated(_)
            | Bytecode::MoveFromDeprecated(_)
            | Bytecode::MoveFromGenericDeprecated(_)
            | Bytecode::MoveToDeprecated(_)
            | Bytecode::MoveToGenericDeprecated(_) => {
                any_write!(buffer, "DEPRECATED BYTECODE: {instruction:?}").map(|_| ())
            }
            // All other instructions are OK to be printed using the standard debug print.
            x => any_write!(buffer, "{x:#?}").map(|_| ()),
        }?;
        let inst_end_offset = buffer.byte_len();
        Ok(Loc::new(
            FileHash::empty(),
            inst_start_offset,
            inst_end_offset,
        ))
    }

    // These need to be in the context of a function or a struct definition since type parameters
    // can refer to function/struct type parameters.
    fn disassemble_sig_tok(
        &self,
        buffer: &mut (impl Write + ByteLength),
        sig_tok: &SignatureToken,
        type_instantiation: Option<&[SignatureToken]>,
        type_param_name_context: &[SourceName],
    ) -> Result<()> {
        match sig_tok {
            SignatureToken::Bool => any_write!(buffer, "bool").map(|_| ()),
            SignatureToken::U8 => any_write!(buffer, "u8").map(|_| ()),
            SignatureToken::U16 => any_write!(buffer, "u16").map(|_| ()),
            SignatureToken::U32 => any_write!(buffer, "u32").map(|_| ()),
            SignatureToken::U64 => any_write!(buffer, "u64").map(|_| ()),
            SignatureToken::U128 => any_write!(buffer, "u128").map(|_| ()),
            SignatureToken::U256 => any_write!(buffer, "u256").map(|_| ()),
            SignatureToken::Address => any_write!(buffer, "address").map(|_| ()),
            SignatureToken::Signer => any_write!(buffer, "signer").map(|_| ()),
            SignatureToken::Datatype(struct_handle_idx) => any_write!(
                buffer,
                "{}",
                self.source_mapper.bytecode.identifier_at(
                    self.source_mapper
                        .bytecode
                        .datatype_handle_at(*struct_handle_idx)
                        .name,
                )
            )
            .map(|_| ()),
            SignatureToken::DatatypeInstantiation(struct_inst) => {
                let (struct_handle_idx, instantiation) = &**struct_inst;
                let name = self.source_mapper.bytecode.identifier_at(
                    self.source_mapper
                        .bytecode
                        .datatype_handle_at(*struct_handle_idx)
                        .name,
                );
                any_write!(buffer, "{name}")?;
                delimited_list(instantiation, "<", ", ", ">", buffer, |buffer, tok| {
                    self.disassemble_sig_tok(
                        buffer,
                        tok,
                        type_instantiation,
                        type_param_name_context,
                    )
                })
            }
            SignatureToken::Vector(sig_tok) => {
                any_write!(buffer, "vector<")?;
                self.disassemble_sig_tok(
                    buffer,
                    sig_tok,
                    type_instantiation,
                    type_param_name_context,
                )?;
                any_write!(buffer, ">").map(|_| ())
            }
            SignatureToken::Reference(sig_tok) => {
                any_write!(buffer, "&")?;
                self.disassemble_sig_tok(
                    buffer,
                    sig_tok,
                    type_instantiation,
                    type_param_name_context,
                )
            }
            SignatureToken::MutableReference(sig_tok) => {
                any_write!(buffer, "&mut ")?;
                self.disassemble_sig_tok(
                    buffer,
                    sig_tok,
                    type_instantiation,
                    type_param_name_context,
                )
            }
            SignatureToken::TypeParameter(ty_param_index) if type_instantiation.is_none() => {
                if let Some(name) = type_param_name_context.get(*ty_param_index as usize) {
                    any_write!(buffer, "{}", name.0)
                } else {
                    any_write!(
                        buffer,
                        "ERROR[Type parameter index {ty_param_index} out of bounds while disassembling type signature]",
                    )
                }.map(|_| ())
            }
            SignatureToken::TypeParameter(ty_param_index) => {
                match type_instantiation.and_then(|i| i.get(*ty_param_index as usize)) {
                    Some(tok) => {
                        self.disassemble_sig_tok(buffer, tok, None, type_param_name_context)
                    }
                    None => {
                        any_write!(
                            buffer,
                            "ERROR[Type parameter index {ty_param_index} out of bounds while disassembling type signature]",
                        ).map(|_| ())
                    }
                }
            }
        }
    }

    fn disassemble_datatype_type_formals(
        buffer: &mut (impl Write + ByteLength),
        source_map_ty_params: &[SourceName],
        type_parameters: &[DatatypeTyParameter],
    ) -> Result<Vec<SourceName>> {
        let mut type_param_source_names = vec![];
        delimited_list(
            source_map_ty_params.iter().zip(type_parameters),
            "<",
            ", ",
            ">",
            buffer,
            |buf, ((name, _), ty_param)| {
                if ty_param.is_phantom {
                    buf.write_str("phantom ")?;
                }
                let type_param_loc = any_write!(buf, "{name}")?;
                type_param_source_names.push((name.to_string(), type_param_loc));
                delimited_list(ty_param.constraints, ": ", " + ", "", buf, |buf, a| {
                    buf.write_str(&Self::format_ability(a))
                        .map_err(anyhow::Error::from)
                })
            },
        )?;
        Ok(type_param_source_names)
    }

    fn disassemble_abilites(
        buffer: &mut (impl Write + ByteLength),
        abilities: AbilitySet,
    ) -> Result<()> {
        if abilities == AbilitySet::EMPTY {
            return Ok(());
        }
        delimited_list(abilities, " has ", ", ", "", buffer, |buf, a| {
            buf.write_str(&Self::format_ability(a))
                .map_err(anyhow::Error::from)
        })
    }

    fn disassemble_fun_type_formals(
        buffer: &mut (impl Write + ByteLength),
        source_map_ty_params: &[SourceName],
        ablities: &[AbilitySet],
    ) -> Result<Vec<SourceName>> {
        let mut type_param_source_names = vec![];
        delimited_list(
            source_map_ty_params.iter().zip(ablities),
            "<",
            ", ",
            ">",
            buffer,
            |buffer, ((name, _), abs)| {
                let type_param_loc = any_write!(buffer, "{name}")?;
                type_param_source_names.push((name.to_string(), type_param_loc));
                delimited_list(*abs, ": ", " + ", "", buffer, |buffer, a| {
                    any_write!(buffer, "{}", Self::format_ability(a)).map(|_| ())
                })
            },
        )?;
        Ok(type_param_source_names)
    }

    fn disassemble_type_for_local(
        &self,
        buffer: &mut (impl Write + ByteLength),
        function_source_map: &FunctionSourceMap,
        local_idx: usize,
        locals: &Signature,
    ) -> Result<()> {
        let Some(sig_tok) = locals.0.get(local_idx) else {
            any_write!(
                buffer,
                "ERROR[Unable to get type for local at index {local_idx}]",
            )?;
            return Ok(());
        };
        self.disassemble_sig_tok(buffer, sig_tok, None, &function_source_map.type_parameters)
    }

    fn disassemble_type_for_parameter_or_local(
        &self,
        buffer: &mut (impl Write + ByteLength),
        function_source_map: &FunctionSourceMap,
        idx: usize,
        parameters: &Signature,
        locals: &Signature,
    ) -> Result<()> {
        let sig_tok = if idx < parameters.len() {
            &parameters.0[idx]
        } else if idx < parameters.len() + locals.len() {
            &locals.0[idx - parameters.len()]
        } else {
            any_write!(
                buffer,
                "ERROR[Unable to get type for parameter or local at index {idx}]",
            )?;
            return Ok(());
        };
        self.disassemble_sig_tok(buffer, sig_tok, None, &function_source_map.type_parameters)
    }

    fn disassemble_type_for_field(
        &self,
        buffer: &mut (impl Write + ByteLength),
        function_source_map: &FunctionSourceMap,
        instantiation: Option<&[SignatureToken]>,
        field_idx: FieldHandleIndex,
    ) -> Result<()> {
        let field_handle = self.source_mapper.bytecode.field_handle_at(field_idx);
        let struct_def = self
            .source_mapper
            .bytecode
            .struct_def_at(field_handle.owner);
        let field_def = match &struct_def.field_information {
            StructFieldInformation::Native => {
                return any_write!(buffer, "ERROR[Attempt to access field on a native struct]")
                    .map(|_| ());
            }
            StructFieldInformation::Declared(fields) => {
                let Some(fields) = fields.get(field_handle.field as usize) else {
                    return any_write!(buffer, "ERROR[Bad field index {}]", field_handle.field)
                        .map(|_| ());
                };
                fields
            }
        };
        let field_type_sig = &field_def.signature.0;
        self.disassemble_sig_tok(
            buffer,
            field_type_sig,
            instantiation,
            &function_source_map.type_parameters,
        )
    }

    fn disassemble_struct_call(
        &self,
        buffer: &mut (impl Write + ByteLength),
        function_source_map: &FunctionSourceMap,
        struct_idx: StructDefinitionIndex,
        signature: &Signature,
    ) -> Result<()> {
        let struct_definition = self.source_mapper.bytecode.struct_def_at(struct_idx);
        let struct_handle = self
            .source_mapper
            .bytecode
            .datatype_handle_at(struct_definition.struct_handle);
        let name = self
            .source_mapper
            .bytecode
            .identifier_at(struct_handle.name)
            .to_string();
        any_write!(buffer, "{name}")?;
        delimited_list(&signature.0, "<", ", ", ">", buffer, |buffer, sig_tok| {
            self.disassemble_sig_tok(buffer, sig_tok, None, &function_source_map.type_parameters)
        })
    }

    fn disassemble_constant(
        &self,
        buffer: &mut (impl Write + ByteLength),
        const_idx: usize,
        constant: &Constant,
        use_inline_formatting: bool,
    ) -> Result<()> {
        let data_str = match try_render_constant(constant) {
            RenderResult::NotRendered => hex::encode(&constant.data),
            RenderResult::AsValue(v_str) => v_str,
            RenderResult::AsString(s) => "\"".to_owned() + &s + "\" // interpreted as UTF8 string",
        };
        if use_inline_formatting {
            self.disassemble_sig_tok(buffer, &constant.type_, None, &[])?;
            any_write!(buffer, ": {}", Self::preview_string(&data_str))
        } else {
            any_write!(buffer, "\t{const_idx} => ")?;
            self.disassemble_sig_tok(buffer, &constant.type_, None, &[])?;
            any_writeln!(buffer, ": {data_str}")
        }
        .map(|_| ())
    }

    fn disassemble_struct_field_access(
        &self,
        buffer: &mut (impl Write + ByteLength),
        field_idx: FieldHandleIndex,
    ) -> Result<()> {
        let field_handle = self.source_mapper.bytecode.field_handle_at(field_idx);
        let struct_def = self
            .source_mapper
            .bytecode
            .struct_def_at(field_handle.owner);
        let field_def = match &struct_def.field_information {
            StructFieldInformation::Native => {
                return any_write!(
                    buffer,
                    "ERROR[Attempt to access field on a native struct {}]",
                    field_idx
                )
                .map(|_| ());
            }
            StructFieldInformation::Declared(fields) => {
                let Some(fields) = fields.get(field_handle.field as usize) else {
                    return any_write!(buffer, "ERROR[Bad field index {}]", field_handle.field)
                        .map(|_| ());
                };
                fields
            }
        };
        let field_name = self
            .source_mapper
            .bytecode
            .identifier_at(field_def.name)
            .to_string();
        let struct_handle = self
            .source_mapper
            .bytecode
            .datatype_handle_at(struct_def.struct_handle);
        let struct_name = self
            .source_mapper
            .bytecode
            .identifier_at(struct_handle.name)
            .to_string();
        any_write!(buffer, "{struct_name}.{field_name}").map(|_| ())
    }

    fn disassemble_function_string(
        &self,
        buffer: &mut (impl Write + ByteLength),
        module_handle: &ModuleHandle,
        function_handle: &FunctionHandle,
    ) -> Result<()> {
        let module_id = self
            .source_mapper
            .bytecode
            .module_id_for_handle(module_handle);
        let function_name = self
            .source_mapper
            .bytecode
            .identifier_at(function_handle.name);
        if self.is_self_id(&module_id) {
            // this is the "self" module. Omit the "module_name::" prefix
            any_write!(buffer, "{function_name}")
        } else {
            let module_name = self
                .module_aliases
                .get(&module_id)
                .cloned()
                .unwrap_or_else(|| module_id.name().to_string());
            any_write!(buffer, "{module_name}::{function_name}")
        }
        .map(|_| ())
    }
}

impl<'a> Disassembler<'a> {
    //***************************************************************************
    // Formatters (that produce formatted strings)
    //***************************************************************************

    fn format_ability(a: Ability) -> String {
        match a {
            Ability::Copy => "copy",
            Ability::Drop => "drop",
            Ability::Store => "store",
            Ability::Key => "key",
        }
        .to_string()
    }

    fn format_name_for_parameter_or_local(
        &self,
        function_source_map: &FunctionSourceMap,
        local_idx: usize,
    ) -> String {
        let Some(name) = function_source_map.get_parameter_or_local_name(local_idx as u64) else {
            return format!(
                "ERROR[Unable to get local name at index {local_idx} while disassembling location-based instruction]"
            );
        };
        name.0
    }

    //***************************************************************************
    // Helpers
    //***************************************************************************

    fn is_self_id(&self, mid: &ModuleId) -> bool {
        &self.source_mapper.bytecode.self_id() == mid
    }

    fn preview_string(s: &str) -> String {
        if s.len() <= PREVIEW_LEN + 2 {
            s.to_string()
        } else {
            let mut preview: String = s.chars().take(PREVIEW_LEN).collect();
            preview.push_str("..");
            preview
        }
    }

    //***************************************************************************
    // Code Coverage Helpers
    //***************************************************************************

    fn get_function_coverage(&self, function_name: &IdentStr) -> Option<&FunctionCoverage> {
        let module = &self.source_mapper.source_map.module_name;

        self.coverage_map.as_ref().and_then(|coverage_map| {
            coverage_map
                .module_maps
                .get(module)
                .and_then(|module_map| module_map.get_function_coverage(function_name))
        })
    }
}
