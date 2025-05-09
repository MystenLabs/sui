// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    compiler_info::CompilerInfo,
    symbols::{
        cursor::CursorContext,
        ide_strings::{
            abilities_to_ide_string, datatype_type_args_to_ide_string, fun_type_to_ide_string,
            mod_ident_to_ide_string, ret_type_to_ide_str, type_args_to_ide_string,
            type_list_to_ide_string, type_to_ide_string, typed_id_list_to_ide_string,
            variant_to_ide_string, visibility_to_ide_string,
        },
        use_def::{References, UseDefMap, UseLoc},
    },
};

use std::{
    cmp,
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::PathBuf,
    sync::Arc,
};

use lsp_types::Position;

use move_command_line_common::files::FileHash;
use move_compiler::{
    command_line::compiler::FullyCompiledProgram,
    editions::Edition,
    expansion::ast::{AbilitySet, ModuleIdent, ModuleIdent_, Visibility},
    naming::ast::{Neighbor, Type},
    parser::ast as P,
    shared::{Name, files::MappedFiles, unique_map::UniqueMap},
    typing::ast::ModuleDefinition,
};
use move_ir_types::location::*;
use move_symbol_pool::Symbol;

/// Information about compiled program (ASTs at different levels)
#[derive(Clone)]
pub struct CompiledProgram {
    pub parsed: P::Program,
    pub typed_modules: UniqueMap<ModuleIdent, ModuleDefinition>,
}

/// Package data used during compilation and analysis
#[derive(Clone)]
pub struct AnalyzedPkgInfo {
    /// Cached fully compiled program representing dependencies
    pub program_deps: Arc<FullyCompiledProgram>,
    /// Cached symbols computation data for dependencies
    pub symbols_data: Option<Arc<SymbolsComputationData>>,
    /// Compiled user program
    pub program: Option<Arc<CompiledProgram>>,
    /// Mapping from file paths to file hashes
    pub file_hashes: Arc<BTreeMap<PathBuf, FileHash>>,
}

/// Information about the compiled package and data structures
/// computed during compilation and analysis
#[derive(Clone)]
pub struct CompiledPkgInfo {
    /// Package path
    pub path: PathBuf,
    /// Manifest hash
    pub manifest_hash: Option<FileHash>,
    /// A combined hash for manifest files of the dependencies
    pub deps_hash: String,
    /// Information about cached dependencies
    pub cached_deps: Option<AnalyzedPkgInfo>,
    /// Compiled user program
    pub program: CompiledProgram,
    /// Maped files
    pub mapped_files: MappedFiles,
    /// Edition of the compiler
    pub edition: Option<Edition>,
    /// Compiler info
    pub compiler_info: Option<CompilerInfo>,
}

/// Data used during symbols computation
#[derive(Clone)]
pub struct SymbolsComputationData {
    /// Outermost definitions in a module (structs, consts, functions), keyed on a ModuleIdent
    /// string
    pub mod_outer_defs: BTreeMap<String, ModuleDefs>,
    /// A UseDefMap for a given module (needs to be appropriately set before the module
    /// processing starts) keyed on a ModuleIdent string
    pub mod_use_defs: BTreeMap<String, UseDefMap>,
    /// Uses (references) for a definition at a given location
    pub references: BTreeMap<Loc, BTreeSet<UseLoc>>,
    /// Additional information about a definitions at a given location
    pub def_info: BTreeMap<Loc, DefInfo>,
    /// Module name lengths in access paths for a given module (needs to be appropriately
    /// set before the module processing starts) keyed on a ModuleIdent string
    pub mod_to_alias_lengths: BTreeMap<String, BTreeMap<Position, usize>>,
}

impl Default for SymbolsComputationData {
    fn default() -> Self {
        Self::new()
    }
}

impl SymbolsComputationData {
    pub fn new() -> Self {
        Self {
            mod_outer_defs: BTreeMap::new(),
            mod_use_defs: BTreeMap::new(),
            references: BTreeMap::new(),
            def_info: BTreeMap::new(),
            mod_to_alias_lengths: BTreeMap::new(),
        }
    }
}

/// Precomputed information about the package and its dependencies
/// cached with the purpose of being re-used during the analysis.
#[derive(Clone)]
pub struct PrecomputedPkgInfo {
    /// Hash of the manifest file for a given package
    pub manifest_hash: Option<FileHash>,
    /// Hash of dependency source files
    pub deps_hash: String,
    /// Precompiled deps
    pub deps: Arc<FullyCompiledProgram>,
    /// Symbols computation data
    pub deps_symbols_data: Arc<SymbolsComputationData>,
    /// Compiled user program
    pub program: Arc<CompiledProgram>,
    /// Mapping from file paths to file hashes
    pub file_hashes: Arc<BTreeMap<PathBuf, FileHash>>,
    /// Edition of the compiler used to build this package
    pub edition: Option<Edition>,
    /// Compiler info
    pub compiler_info: Option<CompilerInfo>,
}

/// Type of a function
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum FunType {
    Macro,
    Entry,
    Regular,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct VariantInfo {
    pub name: Name,
    pub empty: bool,
    pub positional: bool,
}
/// Information about a definition of some identifier
#[derive(Debug, Clone, Eq, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum DefInfo {
    /// Type of an identifier
    Type(Type),
    Function(
        /// Defining module
        ModuleIdent_,
        /// Visibility
        Visibility,
        /// For example, a macro or entry function
        FunType,
        /// Name
        Symbol,
        /// Type args
        Vec<Type>,
        /// Arg names
        Vec<Name>,
        /// Arg types
        Vec<Type>,
        /// Ret type
        Type,
        /// Doc string
        Option<String>,
    ),
    Struct(
        /// Defining module
        ModuleIdent_,
        /// Name
        Symbol,
        /// Visibility
        Visibility,
        /// Type args
        Vec<(Type, bool /* phantom */)>,
        /// Abilities
        AbilitySet,
        /// Field names
        Vec<Name>,
        /// Field types
        Vec<Type>,
        /// Doc string
        Option<String>,
    ),
    Enum(
        /// Defining module
        ModuleIdent_,
        /// Name
        Symbol,
        /// Visibility
        Visibility,
        /// Type args
        Vec<(Type, bool /* phantom */)>,
        /// Abilities
        AbilitySet,
        /// Info about variants
        Vec<VariantInfo>,
        /// Doc string
        Option<String>,
    ),
    Variant(
        /// Defining module of the containing enum
        ModuleIdent_,
        /// Name of the containing enum
        Symbol,
        /// Variant name
        Symbol,
        /// Positional fields?
        bool,
        /// Field names
        Vec<Name>,
        /// Field types
        Vec<Type>,
        /// Doc string
        Option<String>,
    ),
    Field(
        /// Defining module of the containing struct
        ModuleIdent_,
        /// Name of the containing struct
        Symbol,
        /// Field name
        Symbol,
        /// Field type
        Type,
        /// Doc string
        Option<String>,
    ),
    Local(
        /// Name
        Symbol,
        /// Type
        Type,
        /// Should displayed definition be preceded by `let`?
        bool,
        /// Should displayed definition be preceded by `mut`?
        bool,
        /// Location of enum's guard expression (if any) in case
        /// this local definition represents match pattern's variable
        Option<Loc>,
    ),
    Const(
        /// Defining module
        ModuleIdent_,
        /// Name
        Symbol,
        /// Type
        Type,
        /// Value
        Option<String>,
        /// Doc string
        Option<String>,
    ),
    Module(
        /// pkg::mod
        String,
        /// Doc string
        Option<String>,
    ),
}

/// Definition of a struct field
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FieldDef {
    pub name: Symbol,
    pub loc: Loc,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum MemberDefInfo {
    Struct {
        field_defs: Vec<FieldDef>,
        positional: bool,
    },
    Enum {
        variants_info: BTreeMap<Symbol, (Loc, Vec<FieldDef>, /* positional */ bool)>,
    },
    Fun {
        attrs: Vec<String>,
    },
    Const,
}

/// Definition of a module member
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MemberDef {
    pub name_loc: Loc,
    pub info: MemberDefInfo,
}

/// Definition of a local (or parameter)
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LocalDef {
    /// Location of the definition
    pub def_loc: Loc,
    /// Type of definition
    pub def_type: Type,
}

impl PartialOrd for LocalDef {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for LocalDef {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.def_loc.cmp(&other.def_loc)
    }
}

/// Information about call sites relevant to the IDE
#[derive(Debug, Clone, Ord, PartialOrd, PartialEq, Eq)]
pub struct CallInfo {
    /// Is it a dot call?
    pub dot_call: bool,
    /// Locations of arguments
    pub arg_locs: Vec<Loc>,
    /// Definition of function being called (as an Option as its computed after
    /// this struct is created)
    pub def_loc: Option<Loc>,
}

impl CallInfo {
    pub fn new(dot_call: bool, args: &[P::Exp]) -> Self {
        Self {
            dot_call,
            arg_locs: args.iter().map(|e| e.loc).collect(),
            def_loc: None,
        }
    }
}

/// Map from struct name to field order information
pub type StructFieldOrderInfo = BTreeMap<Symbol, BTreeMap<Symbol, usize>>;
/// Map from enum name to variant name to field order information
pub type VariantFieldOrderInfo = BTreeMap<Symbol, BTreeMap<Symbol, BTreeMap<Symbol, usize>>>;

/// Information about field order in structs and enums needed for auto-completion
/// to be consistent with field order in the source code
#[derive(Debug, Clone, Ord, PartialOrd, PartialEq, Eq)]
pub struct FieldOrderInfo {
    pub structs: BTreeMap<String, StructFieldOrderInfo>,
    pub variants: BTreeMap<String, VariantFieldOrderInfo>,
}

impl Default for FieldOrderInfo {
    fn default() -> Self {
        Self::new()
    }
}

impl FieldOrderInfo {
    pub fn new() -> Self {
        Self {
            structs: BTreeMap::new(),
            variants: BTreeMap::new(),
        }
    }
}

/// Module-level definitions and other module-related info
#[derive(Debug, Clone, Copy, Ord, PartialOrd, PartialEq, Eq)]
pub enum AutoImportInsertionKind {
    AfterLastImport,
    BeforeFirstMember, // when no imports exist
}

/// Information needed for auto-import insertion. We do our best
/// to make the insertion fit with what's already in the source file.
/// In particular, if uses are already preasent, we insert the new import
/// in the following line keeping the tabulation of the previous import.
/// If no imports are present, we insert the new import before the first
/// module member (or before its doc comment if it exists), pushing
/// this member down but keeping its original tabulation.
#[derive(Debug, Clone, Copy, Ord, PartialOrd, PartialEq, Eq)]
pub struct AutoImportInsertionInfo {
    // Kind of auto-import insertion
    pub kind: AutoImportInsertionKind,
    // Position in file where insertion should start
    pub pos: Position,
    // Tabulation in number of spaces
    pub tabulation: usize,
}

/// Module-level definitions and other module-related info
#[derive(Debug, Clone, Ord, PartialOrd, PartialEq, Eq)]
pub struct ModuleDefs {
    /// File where this module is located
    pub fhash: FileHash,
    /// Location where this module is located
    pub name_loc: Loc,
    /// Module name
    pub ident: ModuleIdent_,
    /// Struct definitions
    pub structs: BTreeMap<Symbol, MemberDef>,
    /// Enum definitions
    pub enums: BTreeMap<Symbol, MemberDef>,
    /// Const definitions
    pub constants: BTreeMap<Symbol, MemberDef>,
    /// Function definitions
    pub functions: BTreeMap<Symbol, MemberDef>,
    /// Definitions where the type is not explicitly specified
    /// and should be inserted as an inlay hint
    pub untyped_defs: BTreeSet<Loc>,
    /// Information about calls in this module
    pub call_infos: BTreeMap<Loc, CallInfo>,
    /// Position where auto-imports should be inserted
    pub import_insert_info: Option<AutoImportInsertionInfo>,
    /// Dependencies summary
    pub neighbors: UniqueMap<ModuleIdent, Neighbor>,
}

pub type DefMap = BTreeMap<Loc, DefInfo>;
pub type FileUseDefs = BTreeMap<PathBuf, UseDefMap>;
pub type FileModules = BTreeMap<PathBuf, BTreeSet<ModuleDefs>>;

/// Result of the symbolication process
#[derive(Debug, Clone)]
pub struct Symbols {
    /// A map from def locations to all the references (uses)
    pub references: References,
    /// A mapping from uses to definitions in a file
    pub file_use_defs: FileUseDefs,
    /// A mapping from filePath to ModuleDefs
    pub file_mods: FileModules,
    /// Mapped file information for translating locations into positions
    pub files: MappedFiles,
    /// Additional information about definitions
    pub def_info: DefMap,
    /// IDE Annotation Information from the Compiler
    pub compiler_info: CompilerInfo,
    /// Cursor information gathered up during analysis
    pub cursor_context: Option<CursorContext>,
}

impl ModuleDefs {
    pub fn functions(&self) -> &BTreeMap<Symbol, MemberDef> {
        &self.functions
    }

    pub fn structs(&self) -> &BTreeMap<Symbol, MemberDef> {
        &self.structs
    }

    pub fn fhash(&self) -> FileHash {
        self.fhash
    }

    pub fn untyped_defs(&self) -> &BTreeSet<Loc> {
        &self.untyped_defs
    }

    pub fn ident(&self) -> &ModuleIdent_ {
        &self.ident
    }
}

impl fmt::Display for DefInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Type(t) => {
                // Technically, we could use error_format function here to display the "regular"
                // type, but the original intent of this function is subtly different that we need
                // (i.e., to be used by compiler error messages) which, for example, results in
                // verbosity that is not needed here.
                //
                // It also seems like a reasonable idea to be able to tune user experience in the
                // IDE independently on how compiler error messages are generated.
                write!(f, "{}", type_to_ide_string(t, /* verbose */ true))
            }
            Self::Function(
                mod_ident,
                visibility,
                fun_type,
                name,
                type_args,
                arg_names,
                arg_types,
                ret_type,
                _,
            ) => {
                const SINGLE_LINE_TYPE_ARGS_NUM: usize = 2;
                // The strategy for displaying function signature is as follows:
                // - if there are more than SINGLE_LINE_TYPE_ARGS_NUM type args,
                //   they are displayed on separate lines
                // - "regular" args are always displayed on separate lines, which
                //   which is motivated by the fact that datatypes are displayed
                //   in a fully-qualified form (i.e., with package and module name),
                //   and that makes the function name already long and (likely)
                //   the length of each individual type also long (modulo primitive
                //   types of course, but I think we can live with that)
                let type_args_str = type_args_to_ide_string(
                    type_args,
                    /* separate_lines */ type_args.len() > SINGLE_LINE_TYPE_ARGS_NUM,
                    /* verbose */ true,
                );
                let args_str = typed_id_list_to_ide_string(
                    arg_names, arg_types, '(', ')', /* separate_lines */ true,
                    /* verbose */ true,
                );
                let ret_type_str = ret_type_to_ide_str(ret_type, /* verbose */ true);
                write!(
                    f,
                    "{}{}fun {}{}{}{}{}",
                    visibility_to_ide_string(visibility),
                    fun_type_to_ide_string(fun_type),
                    mod_ident_to_ide_string(mod_ident, None, true),
                    name,
                    type_args_str,
                    args_str,
                    ret_type_str,
                )
            }
            Self::Struct(
                mod_ident,
                name,
                visibility,
                type_args,
                abilities,
                field_names,
                field_types,
                _,
            ) => {
                let type_args_str =
                    datatype_type_args_to_ide_string(type_args, /* verbose */ true);
                let abilities_str = abilities_to_ide_string(abilities);
                if field_names.is_empty() {
                    write!(
                        f,
                        "{}struct {}{}{}{} {{}}",
                        visibility_to_ide_string(visibility),
                        mod_ident_to_ide_string(mod_ident, Some(name), true),
                        name,
                        type_args_str,
                        abilities_str,
                    )
                } else {
                    write!(
                        f,
                        "{}struct {}{}{}{} {}",
                        visibility_to_ide_string(visibility),
                        mod_ident_to_ide_string(mod_ident, Some(name), true),
                        name,
                        type_args_str,
                        abilities_str,
                        typed_id_list_to_ide_string(
                            field_names,
                            field_types,
                            '{',
                            '}',
                            /* separate_lines */ true,
                            /* verbose */ true
                        ),
                    )
                }
            }
            Self::Enum(mod_ident, name, visibility, type_args, abilities, variants, _) => {
                let type_args_str =
                    datatype_type_args_to_ide_string(type_args, /* verbose */ true);
                let abilities_str = abilities_to_ide_string(abilities);
                if variants.is_empty() {
                    write!(
                        f,
                        "{}enum {}{}{}{} {{}}",
                        visibility_to_ide_string(visibility),
                        mod_ident_to_ide_string(mod_ident, Some(name), true),
                        name,
                        type_args_str,
                        abilities_str,
                    )
                } else {
                    write!(
                        f,
                        "{}enum {}{}{}{} {{\n{}\n}}",
                        visibility_to_ide_string(visibility),
                        mod_ident_to_ide_string(mod_ident, Some(name), true),
                        name,
                        type_args_str,
                        abilities_str,
                        variant_to_ide_string(variants)
                    )
                }
            }
            Self::Variant(mod_ident, enum_name, name, positional, field_names, field_types, _) => {
                if field_types.is_empty() {
                    write!(
                        f,
                        "{}{}::{}",
                        mod_ident_to_ide_string(mod_ident, Some(enum_name), true),
                        enum_name,
                        name
                    )
                } else if *positional {
                    write!(
                        f,
                        "{}{}::{}({})",
                        mod_ident_to_ide_string(mod_ident, Some(enum_name), true),
                        enum_name,
                        name,
                        type_list_to_ide_string(
                            field_types,
                            /* separate_lines */ false,
                            /* verbose */ true
                        )
                    )
                } else {
                    write!(
                        f,
                        "{}{}::{}{}",
                        mod_ident_to_ide_string(mod_ident, Some(enum_name), true),
                        enum_name,
                        name,
                        typed_id_list_to_ide_string(
                            field_names,
                            field_types,
                            '{',
                            '}',
                            /* separate_lines */ false,
                            /* verbose */ true,
                        ),
                    )
                }
            }
            Self::Field(mod_ident, struct_name, name, t, _) => {
                write!(
                    f,
                    "{}{}\n{}: {}",
                    mod_ident_to_ide_string(mod_ident, Some(struct_name), true),
                    struct_name,
                    name,
                    type_to_ide_string(t, /* verbose */ true)
                )
            }
            Self::Local(name, t, is_decl, is_mut, _) => {
                let mut_str = if *is_mut { "mut " } else { "" };
                if *is_decl {
                    write!(
                        f,
                        "let {}{}: {}",
                        mut_str,
                        name,
                        type_to_ide_string(t, /* verbose */ true)
                    )
                } else {
                    write!(
                        f,
                        "{}{}: {}",
                        mut_str,
                        name,
                        type_to_ide_string(t, /* verbose */ true)
                    )
                }
            }
            Self::Const(mod_ident, name, t, value, _) => {
                if let Some(v) = value {
                    write!(
                        f,
                        "const {}::{}: {} = {}",
                        mod_ident,
                        name,
                        type_to_ide_string(t, /* verbose */ true),
                        v
                    )
                } else {
                    write!(
                        f,
                        "const {}::{}: {}",
                        mod_ident,
                        name,
                        type_to_ide_string(t, /* verbose */ true)
                    )
                }
            }
            Self::Module(mod_ident_str, _) => write!(f, "module {mod_ident_str}"),
        }
    }
}
