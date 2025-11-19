// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    compiler_info::CompilerAnalysisInfo,
    symbols::{
        compilation::{CompiledPkgInfo, ParsedDefinitions, SymbolsComputationData},
        cursor::CursorContext,
        def_info::DefInfo,
        mod_defs::ModuleDefs,
        use_def::{References, UseDef, UseDefMap},
    },
    utils::expansion_mod_ident_to_map_key,
};

use im::ordmap::OrdMap;
use lsp_types::Position;
use move_command_line_common::files::FileHash;
use std::{collections::BTreeMap, sync::Arc};

use move_compiler::{
    expansion::ast::ModuleIdent,
    shared::{NamedAddressMap, files::MappedFiles, unique_map::UniqueMap},
    typing::{ast::ModuleDefinition, visitor::TypingVisitorContext},
};
use move_ir_types::location::Loc;
use move_symbol_pool::Symbol;

pub mod parsing_analysis;
pub mod typing_analysis;

pub type DefMap = BTreeMap<Loc, DefInfo>;

/// Run parsing analysis for either main program or dependencies
pub fn run_parsing_analysis(
    computation_data: &mut SymbolsComputationData,
    compiled_pkg_info: &CompiledPkgInfo,
    cursor_context: Option<&mut CursorContext>,
    parsed_program: &ParsedDefinitions,
    typed_mod_named_address_maps: &BTreeMap<Loc, Arc<NamedAddressMap>>,
) {
    let mut parsing_symbolicator = parsing_analysis::ParsingAnalysisContext {
        mod_outer_defs: &mut computation_data.mod_outer_defs,
        files: &compiled_pkg_info.mapped_files,
        references: &mut computation_data.references,
        def_info: &mut computation_data.def_info,
        use_defs: &mut computation_data.use_defs,
        current_mod_ident_str: None,
        alias_lengths: BTreeMap::new(),
        pkg_addresses: Arc::new(NamedAddressMap::new()),
        cursor: cursor_context,
    };

    parsing_symbolicator.prog_symbols(
        parsed_program,
        &mut computation_data.mod_to_alias_lengths,
        typed_mod_named_address_maps,
    );
}

/// Run typing analysis for either main program or dependencies
pub fn run_typing_analysis(
    mut computation_data: SymbolsComputationData,
    mapped_files: &MappedFiles,
    compiler_analysis_info: &CompilerAnalysisInfo,
    typed_program_modules: &UniqueMap<ModuleIdent, ModuleDefinition>,
) -> SymbolsComputationData {
    let mut typing_symbolicator = typing_analysis::TypingAnalysisContext {
        mod_outer_defs: &mut computation_data.mod_outer_defs,
        files: mapped_files,
        references: &mut computation_data.references,
        def_info: &mut computation_data.def_info,
        use_defs: &mut computation_data.use_defs,
        current_mod_ident_str: None,
        alias_lengths: &BTreeMap::new(),
        traverse_only: false,
        compiler_analysis_info,
        type_params: BTreeMap::new(),
        expression_scope: OrdMap::new(),
    };

    process_typed_modules(
        typed_program_modules,
        &computation_data.mod_to_alias_lengths,
        &mut typing_symbolicator,
    );
    computation_data
}

pub fn find_datatype(mod_defs: &ModuleDefs, datatype_name: &Symbol) -> Option<Loc> {
    mod_defs.structs.get(datatype_name).map_or_else(
        || {
            mod_defs
                .enums
                .get(datatype_name)
                .map(|enum_def| enum_def.name_loc)
        },
        |struct_def| Some(struct_def.name_loc),
    )
}

fn process_typed_modules<'a>(
    typed_modules: &UniqueMap<ModuleIdent, ModuleDefinition>,
    mod_to_alias_lengths: &'a BTreeMap<String, BTreeMap<Position, usize>>,
    typing_symbolicator: &mut typing_analysis::TypingAnalysisContext<'a>,
) {
    for (module_ident, module_def) in typed_modules.key_cloned_iter() {
        let mod_ident_str = expansion_mod_ident_to_map_key(&module_ident.value);
        typing_symbolicator.alias_lengths = mod_to_alias_lengths.get(&mod_ident_str).unwrap();
        typing_symbolicator.visit_module(module_ident, module_def);
    }
}

/// Add use of a function, method, struct or enum identifier
fn add_member_use_def(
    member_def_name: &Symbol, // may be different from use_name for methods
    files: &MappedFiles,
    mod_defs: &ModuleDefs,
    use_name: &Symbol,
    use_loc: &Loc,
    references: &mut References,
    def_info: &DefMap,
    use_defs: &mut BTreeMap<FileHash, UseDefMap>,
    alias_lengths: &BTreeMap<Position, usize>,
) -> Option<UseDef> {
    let Some(name_file_start) = files.start_position_opt(use_loc) else {
        debug_assert!(false);
        return None;
    };
    let name_start = Position {
        line: name_file_start.line_offset() as u32,
        character: name_file_start.column_offset() as u32,
    };
    if let Some(member_def) = mod_defs
        .functions
        .get(member_def_name)
        .or_else(|| mod_defs.structs.get(member_def_name))
        .or_else(|| mod_defs.enums.get(member_def_name))
    {
        let member_info = def_info.get(&member_def.name_loc).unwrap();
        // type def location exists only for structs and enums (and not for functions)
        let ident_type_def_loc = match member_info {
            DefInfo::Struct(_, name, ..) | DefInfo::Enum(_, name, ..) => {
                find_datatype(mod_defs, name)
            }
            _ => None,
        };
        let ud = UseDef::new(
            references,
            alias_lengths,
            use_loc.file_hash(),
            name_start,
            member_def.name_loc,
            use_name,
            ident_type_def_loc,
        );

        use_defs
            .entry(use_loc.file_hash())
            .or_default()
            .insert(name_start.line, ud.clone());
        return Some(ud);
    }
    None
}
