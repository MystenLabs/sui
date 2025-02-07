// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module is responsible for building symbolication information on top of compiler's parsed
//! and typed ASTs, in particular identifier definitions to be used for implementing go-to-def,
//! go-to-references, and on-hover language server commands.
//!
//! The analysis starts with top-level module definitions being processed and then proceeds to
//! process parsed AST (parsing analysis) and typed AST (typing analysis) to gather all the required
//! information which is then summarized in the Symbols struct subsequently used by the language
//! server to find definitions, references, auto-completions, etc.  Parsing analysis is largely
//! responsible for processing import statements (no longer available at the level of typed AST) and
//! typing analysis gathers remaining information. In particular, for local definitions, typing
//! analysis builds a scope stack, entering encountered definitions and matching uses to a
//! definition in the innermost scope.
//!
//! Here is a brief description of how the symbolication information is encoded. Each identifier in
//! the source code of a given module is represented by its location (UseLoc struct): line number,
//! starting and ending column, and hash of the source file where this identifier is located). A
//! definition for each identifier (if any - e.g., built-in type definitions are excluded as there
//! is no place in source code where they are defined) is also represented by its location in the
//! source code (DefLoc struct): line, starting column and a hash of the source file where it's
//! located. The symbolication process maps each identifier with its definition, and also computes
//! other relevant information for each identifier, such as location of its type and information
//! that should be displayed on hover. All this information for an identifier is stored in the
//! UseDef struct.

//! All UseDefs for a given module are stored in a per module map keyed on the line number where the
//! identifier represented by a given UseDef is located - the map entry contains a set of UseDef-s
//! ordered by the column where the identifier starts.
//!
//! For example consider the following code fragment (0-based line numbers on the left and 0-based
//! column numbers at the bottom):
//!
//! 7: const SOME_CONST: u64 = 42;
//! 8:
//! 9: SOME_CONST + SOME_CONST
//!    |     |  |   | |      |
//!    0     6  9  13 15    22
//!
//! Symbolication information for this code fragment would look as follows assuming that this code
//! is stored in a file with hash FHASH (we omit on-hover, type def and doc string info here; also
//! note that identifier in the definition of the constant maps to itself):
//!
//! [7] -> [UseDef(col_start:6,  col_end:13, DefLoc(7:6, FHASH))]
//! [9] -> [UseDef(col_start:0,  col_end: 9, DefLoc(7:6, FHASH))],
//!        [UseDef(col_start:13, col_end:22, DefLoc(7:6, FHASH))]
//!
//! We also associate all uses of an identifier with its definition to support
//! go-to-references. This is done in a global map from an identifier location (DefLoc) to a set of
//! use locations (UseLoc).
#![allow(clippy::non_canonical_partial_ord_impl)]

use crate::{
    analysis::{parsing_analysis, typing_analysis},
    compiler_info::CompilerInfo,
    context::Context,
    diagnostics::{lsp_diagnostics, lsp_empty_diagnostics},
    utils::{loc_start_to_lsp_position_opt, lsp_position_to_loc},
};

use anyhow::{anyhow, Result};
use crossbeam::channel::Sender;
use derivative::*;
use im::ordmap::OrdMap;
use lsp_server::{Request, RequestId};
use lsp_types::{
    request::GotoTypeDefinitionParams, Diagnostic, DocumentSymbol, DocumentSymbolParams,
    GotoDefinitionParams, Hover, HoverContents, HoverParams, Location, MarkupContent, MarkupKind,
    Position, Range, ReferenceParams, SymbolKind,
};

use sha2::{Digest, Sha256};
use std::{
    cmp,
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    thread,
    time::Instant,
    vec,
};
use tempfile::tempdir;
use url::Url;
use vfs::{
    impls::{memory::MemoryFS, overlay::OverlayFS, physical::PhysicalFS},
    VfsPath,
};

use move_command_line_common::files::FileHash;
use move_compiler::{
    command_line::compiler::{construct_pre_compiled_lib, FullyCompiledProgram},
    editions::{Edition, FeatureGate, Flavor},
    expansion::{
        ast::{self as E, AbilitySet, ModuleIdent, ModuleIdent_, Value, Value_, Visibility},
        name_validation::{IMPLICIT_STD_MEMBERS, IMPLICIT_STD_MODULES},
    },
    linters::LintLevel,
    naming::ast::{DatatypeTypeParameter, StructFields, Type, TypeName_, Type_, VariantFields},
    parser::ast::{self as P, DocComment},
    shared::{
        files::MappedFiles, unique_map::UniqueMap, Identifier, Name, NamedAddressMap,
        NamedAddressMaps,
    },
    typing::{
        ast::{Exp, ExpListItem, ModuleDefinition, SequenceItem, SequenceItem_, UnannotatedExp_},
        visitor::TypingVisitorContext,
    },
    unit_test::filter_test_members::UNIT_TEST_POISON_FUN_NAME,
    PASS_CFGIR, PASS_PARSER, PASS_TYPING,
};
use move_core_types::account_address::AccountAddress;
use move_ir_types::location::*;
use move_package::{
    compilation::{build_plan::BuildPlan, compiled_package::ModuleFormat},
    resolution::resolution_graph::ResolvedGraph,
};
use move_symbol_pool::Symbol;

const MANIFEST_FILE_NAME: &str = "Move.toml";
const STD_LIB_PKG_ADDRESS: &str = "0x1";

/// Information about compiled program (ASTs at different levels)
#[derive(Clone)]
pub struct CompiledProgram {
    pub parsed: P::Program,
    pub typed_modules: UniqueMap<ModuleIdent, ModuleDefinition>,
}

/// Package data used during compilation and analysis
#[derive(Clone)]
struct AnalyzedPkgInfo {
    /// Cached fully compiled program representing dependencies
    program_deps: Arc<FullyCompiledProgram>,
    /// Cached symbols computation data for dependencies
    symbols_data: Option<Arc<SymbolsComputationData>>,
    /// Compiled user program
    program: Option<Arc<CompiledProgram>>,
    /// Mapping from file paths to file hashes
    file_hashes: Arc<BTreeMap<PathBuf, FileHash>>,
}

/// Information about the compiled package and data structures
/// computed during compilation and analysis
#[derive(Clone)]
pub struct CompiledPkgInfo {
    /// Package path
    path: PathBuf,
    /// Manifest hash
    manifest_hash: Option<FileHash>,
    /// A combined hash for manifest files of the dependencies
    deps_hash: String,
    /// Information about cached dependencies
    cached_deps: Option<AnalyzedPkgInfo>,
    /// Compiled user program
    program: CompiledProgram,
    /// Maped files
    mapped_files: MappedFiles,
    /// Edition of the compiler
    edition: Option<Edition>,
    /// Compiler info
    compiler_info: Option<CompilerInfo>,
}

/// Data used during symbols computation
#[derive(Clone)]
pub struct SymbolsComputationData {
    /// Outermost definitions in a module (structs, consts, functions), keyed on a ModuleIdent
    /// string
    mod_outer_defs: BTreeMap<String, ModuleDefs>,
    /// A UseDefMap for a given module (needs to be appropriately set before the module
    /// processing starts) keyed on a ModuleIdent string
    mod_use_defs: BTreeMap<String, UseDefMap>,
    /// Uses (references) for a definition at a given location
    references: BTreeMap<Loc, BTreeSet<UseLoc>>,
    /// Additional information about a definitions at a given location
    def_info: BTreeMap<Loc, DefInfo>,
    /// Module name lengths in access paths for a given module (needs to be appropriately
    /// set before the module processing starts) keyed on a ModuleIdent string
    mod_to_alias_lengths: BTreeMap<String, BTreeMap<Position, usize>>,
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
    manifest_hash: Option<FileHash>,
    /// Hash of dependency source files
    deps_hash: String,
    /// Precompiled deps
    deps: Arc<FullyCompiledProgram>,
    /// Symbols computation data
    deps_symbols_data: Arc<SymbolsComputationData>,
    /// Compiled user program
    program: Arc<CompiledProgram>,
    /// Mapping from file paths to file hashes
    file_hashes: Arc<BTreeMap<PathBuf, FileHash>>,
}

/// Location of a use's identifier
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Copy)]
pub struct UseLoc {
    /// File where this use identifier starts
    fhash: FileHash,
    /// Location where this use identifier starts
    start: Position,
    /// Column (on the same line as start)  where this use identifier ends
    col_end: u32,
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

/// Information about both the use identifier (source file is specified wherever an instance of this
/// struct is used) and the definition identifier
#[derive(Debug, Clone, Eq)]
pub struct UseDef {
    /// Column where the (use) identifier location starts on a given line (use this field for
    /// sorting uses on the line)
    col_start: u32,
    /// Column where the (use) identifier location ends on a given line
    col_end: u32,
    /// Location of the definition
    def_loc: Loc,
    /// Location of the type definition
    type_def_loc: Option<Loc>,
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
#[allow(clippy::non_canonical_partial_ord_impl)]
#[derive(Derivative, Debug, Clone, Eq, PartialEq)]
#[derivative(PartialOrd, Ord)]
pub struct LocalDef {
    /// Location of the definition
    pub def_loc: Loc,
    /// Type of definition
    #[derivative(PartialOrd = "ignore")]
    #[derivative(Ord = "ignore")]
    pub def_type: Type,
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
    structs: BTreeMap<String, StructFieldOrderInfo>,
    variants: BTreeMap<String, VariantFieldOrderInfo>,
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
}

#[derive(Clone, Debug)]
pub struct CursorContext {
    /// Set during typing analysis
    pub module: Option<ModuleIdent>,
    /// Set during typing analysis
    pub defn_name: Option<CursorDefinition>,
    // TODO: consider making this a vector to hold the whole chain upward
    /// Set during parsing analysis
    pub position: CursorPosition,
    /// Location provided for the cursor
    pub loc: Loc,
}

#[derive(Clone, Debug, Copy)]
pub enum ChainCompletionKind {
    Type,
    Function,
    All,
}

#[derive(Clone, Debug)]
pub struct ChainInfo {
    pub chain: P::NameAccessChain,
    pub kind: ChainCompletionKind,
    pub inside_use: bool,
}

impl ChainInfo {
    pub fn new(chain: P::NameAccessChain, kind: ChainCompletionKind, inside_use: bool) -> Self {
        Self {
            chain,
            kind,
            inside_use,
        }
    }
}

impl CursorContext {
    fn new(loc: Loc) -> Self {
        CursorContext {
            module: None,
            defn_name: None,
            position: CursorPosition::Unknown,
            loc,
        }
    }

    /// Returns access chain for a match pattern, if any
    fn find_access_chain_in_match_pattern(&self, p: &P::MatchPattern_) -> Option<ChainInfo> {
        use ChainCompletionKind as CT;
        use P::MatchPattern_ as MP;
        match p {
            MP::PositionalConstructor(chain, _) => {
                Some(ChainInfo::new(chain.clone(), CT::Type, false))
            }
            MP::FieldConstructor(chain, _) => Some(ChainInfo::new(chain.clone(), CT::Type, false)),
            MP::Name(_, chain) => Some(ChainInfo::new(chain.clone(), CT::All, false)),
            MP::Literal(_) | MP::Or(..) | MP::At(..) => None,
        }
    }

    /// Returns access chain at cursor position (if any) along with the information of what the chain's
    /// auto-completed target kind should be, and weather it is part of the use statement.
    pub fn find_access_chain(&self) -> Option<ChainInfo> {
        use ChainCompletionKind as CT;
        use CursorPosition as CP;
        match &self.position {
            CP::Exp(sp!(_, exp)) => match exp {
                P::Exp_::Name(chain) if chain.loc.contains(&self.loc) => {
                    return Some(ChainInfo::new(chain.clone(), CT::All, false))
                }
                P::Exp_::Call(chain, _) if chain.loc.contains(&self.loc) => {
                    return Some(ChainInfo::new(chain.clone(), CT::Function, false))
                }
                P::Exp_::Pack(chain, _) if chain.loc.contains(&self.loc) => {
                    return Some(ChainInfo::new(chain.clone(), CT::Type, false))
                }
                _ => (),
            },
            CP::Binding(sp!(_, bind)) => match bind {
                P::Bind_::Unpack(chain, _) if chain.loc.contains(&self.loc) => {
                    return Some(ChainInfo::new(*(chain.clone()), CT::Type, false))
                }
                _ => (),
            },
            CP::Type(sp!(_, ty)) => match ty {
                P::Type_::Apply(chain) if chain.loc.contains(&self.loc) => {
                    return Some(ChainInfo::new(*(chain.clone()), CT::Type, false))
                }
                _ => (),
            },
            CP::Attribute(attr_val) => match &attr_val.value {
                P::AttributeValue_::ModuleAccess(chain) if chain.loc.contains(&self.loc) => {
                    return Some(ChainInfo::new(chain.clone(), CT::All, false))
                }
                _ => (),
            },
            CP::Use(sp!(_, P::Use::Fun { function, ty, .. })) => {
                if function.loc.contains(&self.loc) {
                    return Some(ChainInfo::new(*(function.clone()), CT::Function, true));
                }
                if ty.loc.contains(&self.loc) {
                    return Some(ChainInfo::new(*(ty.clone()), CT::Type, true));
                }
            }
            CP::MatchPattern(sp!(_, p)) => return self.find_access_chain_in_match_pattern(p),
            _ => (),
        };
        None
    }

    /// Returns use declaration at cursor position (if any).
    pub fn find_use_decl(&self) -> Option<P::Use> {
        if let CursorPosition::Use(use_) = &self.position {
            return Some(use_.value.clone());
        }
        None
    }
}

#[derive(Clone, Debug)]
pub enum CursorPosition {
    Exp(P::Exp),
    SeqItem(P::SequenceItem),
    Binding(P::Bind),
    Type(P::Type),
    FieldDefn(P::Field),
    Parameter(P::Var),
    DefName,
    Attribute(P::AttributeValue),
    Use(Spanned<P::Use>),
    MatchPattern(P::MatchPattern),
    Unknown,
    // FIXME: These two are currently unused because these forms don't have enough location
    // recorded on them during parsing.
    DatatypeTypeParameter(P::DatatypeTypeParameter),
    FunctionTypeParameter((Name, Vec<P::Ability>)),
}

#[derive(Clone, Debug)]
pub enum CursorDefinition {
    Function(P::FunctionName),
    Constant(P::ConstantName),
    Struct(P::DatatypeName),
    Enum(P::DatatypeName),
}

type LineOffset = u32;

/// Maps a line number to a list of use-def-s on a given line (use-def set is sorted by col_start)
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UseDefMap(BTreeMap<LineOffset, BTreeSet<UseDef>>);

pub type References = BTreeMap<Loc, BTreeSet<UseLoc>>;
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

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
enum RunnerState {
    Run(BTreeSet<PathBuf>),
    Wait,
    Quit,
}

/// Data used during symbolication running and symbolication info updating
pub struct SymbolicatorRunner {
    mtx_cvar: Arc<(Mutex<RunnerState>, Condvar)>,
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

impl fmt::Display for CursorContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let CursorContext {
            module,
            defn_name,
            position,
            loc: _,
        } = self;
        writeln!(f, "cursor info:")?;
        write!(f, "- module: ")?;
        match module {
            Some(mident) => writeln!(f, "{mident}"),
            None => writeln!(f, "None"),
        }?;
        write!(f, "- definition: ")?;
        match defn_name {
            Some(defn) => match defn {
                CursorDefinition::Function(name) => writeln!(f, "function {name}"),
                CursorDefinition::Constant(name) => writeln!(f, "constant {name}"),
                CursorDefinition::Struct(name) => writeln!(f, "struct {name}"),
                CursorDefinition::Enum(name) => writeln!(f, "enum {name}"),
            },
            None => writeln!(f, "None"),
        }?;
        write!(f, "- position: ")?;
        match position {
            CursorPosition::Attribute(value) => {
                writeln!(f, "attribute value")?;
                writeln!(f, "- value: {:#?}", value)?;
            }
            CursorPosition::Use(value) => {
                writeln!(f, "use value")?;
                writeln!(f, "- value: {:#?}", value)?;
            }
            CursorPosition::DefName => {
                writeln!(f, "defn name")?;
            }
            CursorPosition::Unknown => {
                writeln!(f, "unknown")?;
            }
            CursorPosition::Exp(value) => {
                writeln!(f, "exp")?;
                writeln!(f, "- value: {:#?}", value)?;
            }
            CursorPosition::SeqItem(value) => {
                writeln!(f, "seq item")?;
                writeln!(f, "- value: {:#?}", value)?;
            }
            CursorPosition::Binding(value) => {
                writeln!(f, "binder")?;
                writeln!(f, "- value: {:#?}", value)?;
            }
            CursorPosition::Type(value) => {
                writeln!(f, "type")?;
                writeln!(f, "- value: {:#?}", value)?;
            }
            CursorPosition::FieldDefn(value) => {
                writeln!(f, "field")?;
                writeln!(f, "- value: {:#?}", value)?;
            }
            CursorPosition::Parameter(value) => {
                writeln!(f, "parameter")?;
                writeln!(f, "- value: {:#?}", value)?;
            }
            CursorPosition::MatchPattern(value) => {
                writeln!(f, "match pattern")?;
                writeln!(f, "- value: {:#?}", value)?;
            }
            CursorPosition::DatatypeTypeParameter(value) => {
                writeln!(f, "datatype type param")?;
                writeln!(f, "- value: {:#?}", value)?;
            }
            CursorPosition::FunctionTypeParameter(value) => {
                writeln!(f, "fun type param")?;
                writeln!(f, "- value: {:#?}", value)?;
            }
        }
        Ok(())
    }
}

fn visibility_to_ide_string(visibility: &Visibility) -> String {
    let mut visibility_str = "".to_string();

    if visibility != &Visibility::Internal {
        visibility_str.push_str(format!("{} ", visibility).as_str());
    }
    visibility_str
}

pub fn type_args_to_ide_string(type_args: &[Type], separate_lines: bool, verbose: bool) -> String {
    let mut type_args_str = "".to_string();
    if !type_args.is_empty() {
        type_args_str.push('<');
        if separate_lines {
            type_args_str.push('\n');
        }
        type_args_str.push_str(&type_list_to_ide_string(type_args, separate_lines, verbose));
        if separate_lines {
            type_args_str.push('\n');
        }
        type_args_str.push('>');
    }
    type_args_str
}

fn datatype_type_args_to_ide_string(type_args: &[(Type, bool)], verbose: bool) -> String {
    let mut type_args_str = "".to_string();
    if !type_args.is_empty() {
        type_args_str.push('<');
        type_args_str.push_str(&datatype_type_list_to_ide_string(type_args, verbose));
        type_args_str.push('>');
    }
    type_args_str
}

fn typed_id_list_to_ide_string(
    names: &[Name],
    types: &[Type],
    list_start: char,
    list_end: char,
    separate_lines: bool,
    verbose: bool,
) -> String {
    let list = names
        .iter()
        .zip(types.iter())
        .map(|(n, t)| {
            if separate_lines {
                format!("\t{}: {}", n.value, type_to_ide_string(t, verbose))
            } else {
                format!("{}: {}", n.value, type_to_ide_string(t, verbose))
            }
        })
        .collect::<Vec<_>>()
        .join(if separate_lines { ",\n" } else { ", " });
    if separate_lines && !list.is_empty() {
        format!("{}\n{}\n{}", list_start, list, list_end)
    } else {
        format!("{}{}{}", list_start, list, list_end)
    }
}

pub fn type_to_ide_string(sp!(_, t): &Type, verbose: bool) -> String {
    match t {
        Type_::Unit => "()".to_string(),
        Type_::Ref(m, r) => format!(
            "&{}{}",
            if *m { "mut " } else { "" },
            type_to_ide_string(r, verbose)
        ),
        Type_::Param(tp) => {
            format!("{}", tp.user_specified_name)
        }
        Type_::Apply(_, sp!(_, type_name), ss) => match type_name {
            TypeName_::Multiple(_) => {
                format!(
                    "({})",
                    type_list_to_ide_string(ss, /* separate_lines */ false, verbose)
                )
            }
            TypeName_::Builtin(name) => {
                if ss.is_empty() {
                    format!("{}", name)
                } else {
                    format!(
                        "{}<{}>",
                        name,
                        type_list_to_ide_string(ss, /* separate_lines */ false, verbose)
                    )
                }
            }
            TypeName_::ModuleType(sp!(_, mod_ident), datatype_name) => {
                let type_args = if ss.is_empty() {
                    "".to_string()
                } else {
                    format!(
                        "<{}>",
                        type_list_to_ide_string(ss, /* separate_lines */ false, verbose)
                    )
                };
                if verbose {
                    format!(
                        "{}{}{}",
                        mod_ident_to_ide_string(mod_ident, Some(&datatype_name.value()), true),
                        datatype_name,
                        type_args
                    )
                } else {
                    datatype_name.to_string()
                }
            }
        },
        Type_::Fun(args, ret) => {
            format!(
                "|{}| -> {}",
                type_list_to_ide_string(args, /* separate_lines */ false, verbose),
                type_to_ide_string(ret, verbose)
            )
        }
        Type_::Anything => "_".to_string(),
        Type_::Var(_) => "invalid type (var)".to_string(),
        Type_::UnresolvedError => "unknown type (unresolved)".to_string(),
    }
}

pub fn type_list_to_ide_string(types: &[Type], separate_lines: bool, verbose: bool) -> String {
    types
        .iter()
        .map(|t| {
            if separate_lines {
                format!("\t{}", type_to_ide_string(t, verbose))
            } else {
                type_to_ide_string(t, verbose)
            }
        })
        .collect::<Vec<_>>()
        .join(if separate_lines { ",\n" } else { ", " })
}

fn datatype_type_list_to_ide_string(types: &[(Type, bool)], verbose: bool) -> String {
    types
        .iter()
        .map(|(t, phantom)| {
            if *phantom {
                format!("phantom {}", type_to_ide_string(t, verbose))
            } else {
                type_to_ide_string(t, verbose)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn ret_type_to_ide_str(ret_type: &Type, verbose: bool) -> String {
    match ret_type {
        sp!(_, Type_::Unit) => "".to_string(),
        _ => format!(": {}", type_to_ide_string(ret_type, verbose)),
    }
}
/// Conversions of constant values to strings is currently best-effort which is why this function
/// returns an Option (in the worst case we will display constant name and type but no value).
fn const_val_to_ide_string(exp: &Exp) -> Option<String> {
    ast_exp_to_ide_string(exp)
}

fn ast_exp_to_ide_string(exp: &Exp) -> Option<String> {
    use UnannotatedExp_ as UE;
    let sp!(_, e) = &exp.exp;
    match e {
        UE::Constant(mod_ident, name) => Some(format!("{mod_ident}::{name}")),
        UE::Value(v) => Some(ast_value_to_ide_string(v)),
        UE::Vector(_, _, _, exp) => ast_exp_to_ide_string(exp).map(|s| format!("[{s}]")),
        UE::Block((_, seq)) | UE::NamedBlock(_, (_, seq)) => {
            let seq_items = seq
                .iter()
                .map(ast_seq_item_to_ide_string)
                .collect::<Vec<_>>();
            if seq_items.iter().any(|o| o.is_none()) {
                // even if only one element cannot be turned into string, don't try displaying block content at all
                return None;
            }
            Some(
                seq_items
                    .into_iter()
                    .map(|o| o.unwrap())
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        }
        UE::ExpList(list) => {
            let items = list
                .iter()
                .map(|i| match i {
                    ExpListItem::Single(exp, _) => ast_exp_to_ide_string(exp),
                    ExpListItem::Splat(_, exp, _) => ast_exp_to_ide_string(exp),
                })
                .collect::<Vec<_>>();
            if items.iter().any(|o| o.is_none()) {
                // even if only one element cannot be turned into string, don't try displaying expression list at all
                return None;
            }
            Some(
                items
                    .into_iter()
                    .map(|o| o.unwrap())
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        }
        UE::UnaryExp(op, exp) => ast_exp_to_ide_string(exp).map(|s| format!("{op}{s}")),

        UE::BinopExp(lexp, op, _, rexp) => {
            let Some(ls) = ast_exp_to_ide_string(lexp) else {
                return None;
            };
            let Some(rs) = ast_exp_to_ide_string(rexp) else {
                return None;
            };
            Some(format!("{ls} {op} {rs}"))
        }
        _ => None,
    }
}

fn ast_seq_item_to_ide_string(sp!(_, seq_item): &SequenceItem) -> Option<String> {
    use SequenceItem_ as SI;
    match seq_item {
        SI::Seq(exp) => ast_exp_to_ide_string(exp),
        _ => None,
    }
}

fn ast_value_to_ide_string(sp!(_, val): &Value) -> String {
    use Value_ as V;
    match val {
        V::Address(addr) => format!("@{}", addr),
        V::InferredNum(u) => format!("{}", u),
        V::U8(u) => format!("{}", u),
        V::U16(u) => format!("{}", u),
        V::U32(u) => format!("{}", u),
        V::U64(u) => format!("{}", u),
        V::U128(u) => format!("{}", u),
        V::U256(u) => format!("{}", u),
        V::Bool(b) => format!("{}", b),
        V::Bytearray(vec) => format!(
            "[{}]",
            vec.iter()
                .map(|v| format!("{}", v))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// Creates a string representing a module ID, either on it's owne as in `pkg::module`
/// or as part of a datatype or function type, in which it should be `pkg::module::`.
/// If it's part of the datatype, name of the datatype is passed in `datatype_name_opt`.
pub fn mod_ident_to_ide_string(
    mod_ident: &ModuleIdent_,
    datatype_name_opt: Option<&Symbol>,
    is_access_chain_prefix: bool, // part of access chaing that should end with `::`
) -> String {
    use E::Address as A;
    // the module ID is to be a prefix to a data
    let suffix = if is_access_chain_prefix { "::" } else { "" };
    match mod_ident.address {
        A::Numerical { name, value, .. } => {
            let pkg_name = match name {
                Some(n) => n.to_string(),
                None => value.to_string(),
            };

            let Ok(std_lib_pkg_address) = AccountAddress::from_hex_literal(STD_LIB_PKG_ADDRESS)
            else {
                // getting stdlib address did not work - use the whole thing
                return format!("{pkg_name}::{}{}", mod_ident.module, suffix);
            };
            if value.value.into_inner() != std_lib_pkg_address {
                // it's not a stdlib package - use the whole thing
                return format!("{pkg_name}::{}{}", mod_ident.module, suffix);
            }
            // try stripping both package and module if this conversion
            // is for a datatype, oherwise try only stripping package
            if let Some(datatype_name) = datatype_name_opt {
                if IMPLICIT_STD_MEMBERS.iter().any(
                    |(implicit_mod_name, implicit_datatype_name, _)| {
                        mod_ident.module.value() == *implicit_mod_name
                            && datatype_name == implicit_datatype_name
                    },
                ) {
                    // strip both package and module (whether its meant to be
                    // part of access chain or not, if there is not module,
                    // there should be no `::` at the end)
                    return "".to_string();
                }
            }
            if IMPLICIT_STD_MODULES
                .iter()
                .any(|implicit_mod_name| mod_ident.module.value() == *implicit_mod_name)
            {
                // strip package
                return format!("{}{}", mod_ident.module.value(), suffix);
            }
            // stripping prefix didn't work - use the whole thing
            format!("{pkg_name}::{}{}", mod_ident.module, suffix)
        }
        A::NamedUnassigned(n) => format!("{n}::{}", mod_ident.module).to_string(),
    }
}

fn fun_type_to_ide_string(fun_type: &FunType) -> String {
    match fun_type {
        FunType::Entry => "entry ",
        FunType::Macro => "macro ",
        FunType::Regular => "",
    }
    .to_string()
}

fn abilities_to_ide_string(abilities: &AbilitySet) -> String {
    if abilities.is_empty() {
        "".to_string()
    } else {
        format!(
            " has {}",
            abilities
                .iter()
                .map(|a| format!("{a}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

fn variant_to_ide_string(variants: &[VariantInfo]) -> String {
    // how many variant lines (including optional ellipsis if there
    // are too many of them) are printed
    const NUM_PRINTED: usize = 7;
    let mut vstrings = variants
        .iter()
        .enumerate()
        .map(|(idx, info)| {
            if idx >= NUM_PRINTED - 1 {
                "\t/* ... */".to_string()
            } else if info.empty {
                format!("\t{}", info.name)
            } else if info.positional {
                format!("\t{}( /* ... */ )", info.name)
            } else {
                format!("\t{}{{ /* ... */ }}", info.name)
            }
        })
        .collect::<Vec<_>>();
    vstrings.truncate(NUM_PRINTED);
    vstrings.join(",\n")
}

impl SymbolicatorRunner {
    /// Create a new idle runner (one that does not actually symbolicate)
    pub fn idle() -> Self {
        let mtx_cvar = Arc::new((Mutex::new(RunnerState::Wait), Condvar::new()));
        SymbolicatorRunner { mtx_cvar }
    }

    /// Create a new runner
    pub fn new(
        ide_files_root: VfsPath,
        symbols_map: Arc<Mutex<BTreeMap<PathBuf, Symbols>>>,
        packages_info: Arc<Mutex<BTreeMap<PathBuf, PrecomputedPkgInfo>>>,
        sender: Sender<Result<BTreeMap<PathBuf, Vec<Diagnostic>>>>,
        lint: LintLevel,
    ) -> Self {
        let mtx_cvar = Arc::new((Mutex::new(RunnerState::Wait), Condvar::new()));
        let thread_mtx_cvar = mtx_cvar.clone();
        let runner = SymbolicatorRunner { mtx_cvar };

        thread::Builder::new()
            .spawn(move || {
                let (mtx, cvar) = &*thread_mtx_cvar;
                // Locations opened in the IDE (files or directories) for which manifest file is missing
                let mut missing_manifests = BTreeSet::new();
                // infinite loop to wait for symbolication requests
                eprintln!("starting symbolicator runner loop");
                loop {
                    let starting_paths_opt = {
                        // hold the lock only as long as it takes to get the data, rather than through
                        // the whole symbolication process (hence a separate scope here)
                        let mut symbolicate = mtx.lock().unwrap();
                        match symbolicate.clone() {
                            RunnerState::Quit => break,
                            RunnerState::Run(starting_paths) => {
                                *symbolicate = RunnerState::Wait;
                                Some(starting_paths)
                            }
                            RunnerState::Wait => {
                                // wait for next request
                                symbolicate = cvar.wait(symbolicate).unwrap();
                                match symbolicate.clone() {
                                    RunnerState::Quit => break,
                                    RunnerState::Run(starting_paths) => {
                                        *symbolicate = RunnerState::Wait;
                                        Some(starting_paths)
                                    }
                                    RunnerState::Wait => None,
                                }
                            }
                        }
                    };
                    if let Some(starting_paths) = starting_paths_opt {
                        // aggregate all starting paths by package
                        let pkgs_to_analyze = Self::pkgs_to_analyze(
                            starting_paths,
                            &mut missing_manifests,
                            sender.clone(),
                        );
                        for (pkg_path, modified_files) in pkgs_to_analyze.into_iter() {
                            eprintln!("symbolication started");
                            match get_symbols(
                                packages_info.clone(),
                                ide_files_root.clone(),
                                pkg_path.as_path(),
                                Some(modified_files.into_iter().collect()),
                                lint,
                                None,
                            ) {
                                Ok((symbols_opt, lsp_diagnostics)) => {
                                    eprintln!("symbolication finished");
                                    if let Some(new_symbols) = symbols_opt {
                                        // replace symbolication info for a given package
                                        //
                                        // TODO: we may consider "unloading" symbolication information when
                                        // files/directories are being closed but as with other performance
                                        // optimizations (e.g. incrementalizatino of the vfs), let's wait
                                        // until we know we actually need it
                                        let mut old_symbols_map = symbols_map.lock().unwrap();
                                        old_symbols_map.insert(pkg_path.clone(), new_symbols);
                                    }
                                    // set/reset (previous) diagnostics
                                    if let Err(err) = sender.send(Ok(lsp_diagnostics)) {
                                        eprintln!("could not pass diagnostics: {:?}", err);
                                    }
                                }
                                Err(err) => {
                                    eprintln!("symbolication failed: {:?}", err);
                                    if let Err(err) = sender.send(Err(err)) {
                                        eprintln!("could not pass compiler error: {:?}", err);
                                    }
                                }
                            }
                        }
                    }
                }
            })
            .unwrap();

        runner
    }

    /// Aggregates all starting paths by package
    fn pkgs_to_analyze(
        starting_paths: BTreeSet<PathBuf>,
        missing_manifests: &mut BTreeSet<PathBuf>,
        sender: Sender<Result<BTreeMap<PathBuf, Vec<Diagnostic>>>>,
    ) -> BTreeMap<PathBuf, BTreeSet<PathBuf>> {
        let mut pkgs_to_analyze: BTreeMap<PathBuf, BTreeSet<PathBuf>> = BTreeMap::new();
        for starting_path in &starting_paths {
            let Some(root_dir) = Self::root_dir(starting_path) else {
                if !missing_manifests.contains(starting_path) {
                    eprintln!("reporting missing manifest");
                    // report missing manifest file only once to avoid cluttering IDE's UI in
                    // cases when developer indeed intended to open a standalone file that was
                    // not meant to compile
                    missing_manifests.insert(starting_path.clone());
                    if let Err(err) = sender.send(Err(anyhow!(
                        "Unable to find package manifest. Make sure that
                    the source files are located in a sub-directory of a package containing
                    a Move.toml file. "
                    ))) {
                        eprintln!("could not pass missing manifest error: {:?}", err);
                    }
                }
                continue;
            };
            // The mutext value is only set by the `on_text_document_sync_notification` handler
            // and can only contain a valid Move file path, so we simply collect a set of Move
            // file paths here to pass them to the symbolicator.
            let modfied_files = pkgs_to_analyze.entry(root_dir.clone()).or_default();
            modfied_files.insert(starting_path.clone());
        }
        pkgs_to_analyze
    }

    pub fn run(&self, starting_path: PathBuf) {
        eprintln!("scheduling run for {:?}", starting_path);
        let (mtx, cvar) = &*self.mtx_cvar;
        let mut symbolicate = mtx.lock().unwrap();
        match symbolicate.clone() {
            RunnerState::Quit => (), // do nothing as we are quitting
            RunnerState::Run(mut all_starting_paths) => {
                all_starting_paths.insert(starting_path);
                *symbolicate = RunnerState::Run(all_starting_paths);
            }
            RunnerState::Wait => {
                let mut all_starting_paths = BTreeSet::new();
                all_starting_paths.insert(starting_path);
                *symbolicate = RunnerState::Run(all_starting_paths);
            }
        }
        cvar.notify_one();
        eprintln!("scheduled run");
    }

    pub fn quit(&self) {
        let (mtx, cvar) = &*self.mtx_cvar;
        let mut symbolicate = mtx.lock().unwrap();
        *symbolicate = RunnerState::Quit;
        cvar.notify_one();
    }

    /// Finds manifest file in a (sub)directory of the starting path passed as argument
    pub fn root_dir(starting_path: &Path) -> Option<PathBuf> {
        let mut current_path_opt = Some(starting_path);
        while current_path_opt.is_some() {
            let current_path = current_path_opt.unwrap();
            let manifest_path = current_path.join(MANIFEST_FILE_NAME);
            if manifest_path.is_file() {
                return Some(current_path.to_path_buf());
            }
            current_path_opt = current_path.parent();
        }
        None
    }
}

impl UseDef {
    pub fn new(
        references: &mut References,
        alias_lengths: &BTreeMap<Position, usize>,
        use_fhash: FileHash,
        use_start: Position,
        def_loc: Loc,
        use_name: &Symbol,
        type_def_loc: Option<Loc>,
    ) -> Self {
        // Normally, we compute the length of the identifier as the length
        // of the string that represents it as this string is the same
        // in the source file and in the AST. However, for aliased module
        // accesses, the string in the source represents the alias and
        // the string in the AST represents the actual (non-aliased) module
        // name - we need to retrieve the correct source-level length
        // from the map, otherwise on-hover may not work correctly
        // if AST-level and source-level lengths are different.
        //
        // To illustrate it with an example, in the source we may have:
        //
        // module Symbols::M9 {
        //     use Symbols::M1 as ALIAS_M1;
        //
        //    struct SomeStruct  {
        //        some_field: ALIAS_M1::AnotherStruct,
        //    }
        // }
        //
        // In the (typed) AST we will however have:
        //
        // module Symbols::M9 {
        //     use Symbols::M1 as ALIAS_M1;
        //
        //    struct SomeStruct  {
        //        some_field: M1::AnotherStruct,
        //    }
        // }
        //
        // As a result, when trying to connect the "use" of module alias with
        // the module definition, at the level of (typed) AST we will have
        // identifier of the wrong length which may mess up on-hover and go-to-default
        // (hovering over a portion of a longer alias may not trigger either).

        let use_name_len = match alias_lengths.get(&use_start) {
            Some(l) => *l,
            None => use_name.len(),
        };
        let col_end = use_start.character + use_name_len as u32;
        let use_loc = UseLoc {
            fhash: use_fhash,
            start: use_start,
            col_end,
        };

        references.entry(def_loc).or_default().insert(use_loc);
        Self {
            col_start: use_start.character,
            col_end,
            def_loc,
            type_def_loc,
        }
    }

    /// Given a UseDef, modify just the use name and location (to make it represent an alias).
    pub fn rename_use(
        &mut self,
        references: &mut References,
        new_name: Symbol,
        new_start: Position,
        new_fhash: FileHash,
    ) {
        self.col_start = new_start.character;
        self.col_end = new_start.character + new_name.len() as u32;
        let new_use_loc = UseLoc {
            fhash: new_fhash,
            start: new_start,
            col_end: self.col_end,
        };

        references
            .entry(self.def_loc)
            .or_default()
            .insert(new_use_loc);
    }

    pub fn col_start(&self) -> u32 {
        self.col_start
    }

    pub fn col_end(&self) -> u32 {
        self.col_end
    }

    pub fn def_loc(&self) -> Loc {
        self.def_loc
    }

    // use_line is zero-indexed
    pub fn render(
        &self,
        f: &mut dyn std::io::Write,
        symbols: &Symbols,
        use_line: u32,
        use_file_content: &str,
        def_file_content: &str,
    ) -> std::io::Result<()> {
        let UseDef {
            col_start,
            col_end,
            def_loc,
            type_def_loc,
        } = self;
        let uident = use_ident(use_file_content, use_line, *col_start, *col_end);
        writeln!(f, "Use: '{uident}', start: {col_start}, end: {col_end}")?;
        let dstart = symbols.files.start_position(def_loc);
        let dline = dstart.line_offset() as u32;
        let dcharacter = dstart.column_offset() as u32;
        let dident = def_ident(def_file_content, dline, dcharacter);
        writeln!(f, "Def: '{dident}', line: {dline}, def char: {dcharacter}")?;
        if let Some(ty_loc) = type_def_loc {
            let tdstart = symbols.files.start_position(ty_loc);
            let tdline = tdstart.line_offset() as u32;
            let tdcharacter = tdstart.column_offset() as u32;
            if let Some((_, type_def_file_content)) = symbols.files.get(&ty_loc.file_hash()) {
                let type_dident = def_ident(&type_def_file_content, tdline, tdcharacter);
                writeln!(
                    f,
                    "TypeDef: '{type_dident}', line: {tdline}, char: {tdcharacter}"
                )
            } else {
                writeln!(f, "TypeDef: INCORRECT INFO")
            }
        } else {
            writeln!(f, "TypeDef: no info")
        }
    }
}

fn use_ident(use_file_content: &str, use_line: u32, col_start: u32, col_end: u32) -> String {
    if let Some(line) = use_file_content.lines().nth(use_line as usize) {
        if let Some((start, _)) = line.char_indices().nth(col_start as usize) {
            if let Some((end, _)) = line.char_indices().nth(col_end as usize) {
                return line[start..end].into();
            }
        }
    }
    "INVALID USE IDENT".to_string()
}

fn def_ident(def_file_content: &str, def_line: u32, col_start: u32) -> String {
    if let Some(line) = def_file_content.lines().nth(def_line as usize) {
        if let Some((start, _)) = line.char_indices().nth(col_start as usize) {
            let end = line[start..]
                .char_indices()
                .find(|(_, c)| !c.is_alphanumeric() && *c != '_' && *c != '$')
                .map_or(line.len(), |(i, _)| start + i);
            return line[start..end].into();
        }
    }
    "INVALID DEF IDENT".to_string()
}

impl Ord for UseDef {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.col_start.cmp(&other.col_start)
    }
}

impl PartialOrd for UseDef {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for UseDef {
    fn eq(&self, other: &Self) -> bool {
        self.col_start == other.col_start
    }
}

impl Default for UseDefMap {
    fn default() -> Self {
        Self::new()
    }
}

impl UseDefMap {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub fn insert(&mut self, key: u32, val: UseDef) {
        self.0.entry(key).or_default().insert(val);
    }

    pub fn get(&self, key: u32) -> Option<BTreeSet<UseDef>> {
        self.0.get(&key).cloned()
    }

    pub fn elements(self) -> BTreeMap<u32, BTreeSet<UseDef>> {
        self.0
    }

    pub fn count(&self) -> usize {
        self.0.len()
    }

    pub fn extend(&mut self, use_defs: BTreeMap<u32, BTreeSet<UseDef>>) {
        for (k, v) in use_defs {
            self.0.entry(k).or_default().extend(v);
        }
    }
}

impl Symbols {
    pub fn line_uses(&self, use_fpath: &Path, use_line: u32) -> BTreeSet<UseDef> {
        let Some(file_symbols) = self.file_use_defs.get(use_fpath) else {
            return BTreeSet::new();
        };
        file_symbols.get(use_line).unwrap_or_else(BTreeSet::new)
    }

    pub fn def_info(&self, def_loc: &Loc) -> Option<&DefInfo> {
        self.def_info.get(def_loc)
    }

    pub fn mod_defs(&self, fhash: &FileHash, mod_ident: ModuleIdent_) -> Option<&ModuleDefs> {
        let Some(fpath) = self.files.file_name_mapping().get(fhash) else {
            return None;
        };
        let Some(mod_defs) = self.file_mods.get(fpath) else {
            return None;
        };
        mod_defs.iter().find(|d| d.ident == mod_ident)
    }

    pub fn file_hash(&self, path: &Path) -> Option<FileHash> {
        let Some(mod_defs) = self.file_mods.get(path) else {
            return None;
        };
        Some(mod_defs.first().unwrap().fhash)
    }
}

fn has_precompiled_deps(
    pkg_path: &Path,
    pkg_dependencies: Arc<Mutex<BTreeMap<PathBuf, PrecomputedPkgInfo>>>,
) -> bool {
    let pkg_deps = pkg_dependencies.lock().unwrap();
    pkg_deps.contains_key(pkg_path)
}

/// Checks if a hash is included in the file hashes list.
/// We only consider file hashes from files.
fn hash_included_in_file_hashes(
    hash: FileHash,
    modified_files: &BTreeSet<PathBuf>,
    file_hashes: Arc<BTreeMap<PathBuf, FileHash>>,
) -> bool {
    modified_files.iter().any(|fpath| {
        file_hashes.get(fpath).map_or_else(
            || {
                debug_assert!(false);
                false
            },
            |fhash| hash == *fhash,
        )
    })
}

/// Checks if a parsed module has been modified by comparing
/// file hash in the module with the file hashes provided
/// as an argument to see if module hash is included in the
/// hashes provided. We only consider file hashes from modified
/// files.
fn is_parsed_mod_modified(
    mdef: &P::ModuleDefinition,
    modified_files: &BTreeSet<PathBuf>,
    file_hashes: Arc<BTreeMap<PathBuf, FileHash>>,
) -> bool {
    !hash_included_in_file_hashes(mdef.loc.file_hash(), modified_files, file_hashes)
}

/// Checks if a typed module has been modified by comparing
/// file hash in the module with the file hashes provided
/// as an argument to see if module hash is included in the
/// hashes provided. We only consider file hashes from modified
/// files.
fn is_typed_mod_modified(
    mdef: &ModuleDefinition,
    modified_files: &BTreeSet<PathBuf>,
    file_hashes: Arc<BTreeMap<PathBuf, FileHash>>,
) -> bool {
    !hash_included_in_file_hashes(mdef.loc.file_hash(), modified_files, file_hashes)
}

/// Checks if a parsed package has been modified by comparing
/// file hash in the package's modules with the file hashes provided
/// as an argument to see if all module hashes are included
/// in the hashes provided. We only consider file hashes from modified
/// files.
fn is_parsed_pkg_modified(
    pkg_def: &P::PackageDefinition,
    modified_files: &BTreeSet<PathBuf>,
    file_hashes: Arc<BTreeMap<PathBuf, FileHash>>,
) -> bool {
    match &pkg_def.def {
        P::Definition::Module(mdef) => is_parsed_mod_modified(mdef, modified_files, file_hashes),
        P::Definition::Address(adef) => adef
            .modules
            .iter()
            .any(|mdef| is_parsed_mod_modified(mdef, modified_files, file_hashes.clone())),
    }
}

/// Merges a cached compiled program with newly computed compiled program
/// In the newly computed program, only modified files are fully compiled
/// and these files are merged with the cached compiled program.
fn merge_user_programs(
    cached_info_opt: Option<AnalyzedPkgInfo>,
    parsed_program_new: P::Program,
    typed_program_modules_new: UniqueMap<ModuleIdent, ModuleDefinition>,
    file_hashes_new: Arc<BTreeMap<PathBuf, FileHash>>,
    files_to_compile: BTreeSet<PathBuf>,
) -> (P::Program, UniqueMap<ModuleIdent, ModuleDefinition>) {
    // unraps are safe as this function only called when cached compiled program exists
    let cached_info = cached_info_opt.unwrap();
    let compiled_program = cached_info.program.unwrap();
    let file_hashes_cached = cached_info.file_hashes;
    let mut parsed_program_cached = compiled_program.parsed.clone();
    let mut typed_modules_cached = compiled_program.typed_modules.clone();
    // address maps might have changed but all would be computed in full during
    // incremental compilation as only function bodies are omitted
    parsed_program_cached.named_address_maps = parsed_program_new.named_address_maps;
    // remove modules from user code that belong to modified files (use new
    // file hashes - if cached module's hash is on the list of new file hashes, it means
    // that nothing changed)
    parsed_program_cached.source_definitions.retain(|pkg_def| {
        !is_parsed_pkg_modified(pkg_def, &files_to_compile, file_hashes_new.clone())
    });
    let mut typed_modules_cached_filtered = UniqueMap::new();
    for (mident, mdef) in typed_modules_cached.into_iter() {
        if !is_typed_mod_modified(&mdef, &files_to_compile, file_hashes_new.clone()) {
            _ = typed_modules_cached_filtered.add(mident, mdef);
        }
    }
    typed_modules_cached = typed_modules_cached_filtered;
    // add new modules from user code (use cached file hashes - if new module's hash is on the list of
    // cached file hashes, it means that nothing' changed)
    for pkg_def in parsed_program_new.source_definitions {
        if is_parsed_pkg_modified(&pkg_def, &files_to_compile, file_hashes_cached.clone()) {
            parsed_program_cached.source_definitions.push(pkg_def);
        }
    }
    for (mident, mdef) in typed_program_modules_new.into_iter() {
        if is_typed_mod_modified(&mdef, &files_to_compile, file_hashes_cached.clone()) {
            typed_modules_cached.remove(&mident); // in case new file has new definition of the module
            _ = typed_modules_cached.add(mident, mdef);
        }
    }

    (parsed_program_cached, typed_modules_cached)
}

/// Builds a package at a given path and, if successful, returns parsed AST
/// and typed AST as well as (regardless of success) diagnostics.
/// See `get_symbols` for explanation of what `modified_files` parameter is.
pub fn get_compiled_pkg(
    packages_info: Arc<Mutex<BTreeMap<PathBuf, PrecomputedPkgInfo>>>,
    ide_files_root: VfsPath,
    pkg_path: &Path,
    modified_files: Option<Vec<PathBuf>>,
    lint: LintLevel,
) -> Result<(Option<CompiledPkgInfo>, BTreeMap<PathBuf, Vec<Diagnostic>>)> {
    let build_config = move_package::BuildConfig {
        test_mode: true,
        install_dir: Some(tempdir().unwrap().path().to_path_buf()),
        default_flavor: Some(Flavor::Sui),
        lint_flag: lint.into(),
        skip_fetch_latest_git_deps: has_precompiled_deps(pkg_path, packages_info.clone()),
        ..Default::default()
    };

    eprintln!("symbolicating {:?}", pkg_path);

    // resolution graph diagnostics are only needed for CLI commands so ignore them by passing a
    // vector as the writer
    let resolution_graph =
        build_config.resolution_graph_for_package(pkg_path, None, &mut Vec::new())?;
    let root_pkg_name = resolution_graph.graph.root_package_name;

    let overlay_fs_root = VfsPath::new(OverlayFS::new(&[
        VfsPath::new(MemoryFS::new()),
        ide_files_root.clone(),
        VfsPath::new(PhysicalFS::new("/")),
    ]));

    let manifest_file = overlay_fs_root
        .join(pkg_path.to_string_lossy())
        .and_then(|p| p.join(MANIFEST_FILE_NAME))
        .and_then(|p| p.open_file());

    let manifest_hash = if let Ok(mut f) = manifest_file {
        let mut contents = String::new();
        let _ = f.read_to_string(&mut contents);
        Some(FileHash::new(&contents))
    } else {
        None
    };

    // Hash dependencies so we can check if something has changed.
    let (mapped_files, deps_hash) =
        compute_mapped_files(&resolution_graph, overlay_fs_root.clone());
    let file_hashes: Arc<BTreeMap<PathBuf, FileHash>> = Arc::new(
        mapped_files
            .file_name_mapping()
            .iter()
            .map(|(fhash, fpath)| (fpath.clone(), *fhash))
            .collect(),
    );
    let compiler_flags = resolution_graph.build_options.compiler_flags().clone();
    let build_plan =
        BuildPlan::create(resolution_graph)?.set_compiler_vfs_root(overlay_fs_root.clone());
    let mut parsed_ast = None;
    let mut typed_ast = None;
    let mut compiler_info = None;
    let mut diagnostics = None;

    let mut dependencies = build_plan.compute_dependencies();
    let cached_info_opt = if let Ok(deps_package_paths) = dependencies.make_deps_for_compiler() {
        // Partition deps_package according whether src is available
        let src_deps = deps_package_paths
            .iter()
            .filter_map(|(p, b)| {
                if let ModuleFormat::Source = b {
                    Some(p.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let src_names = src_deps
            .iter()
            .filter_map(|p| p.name.as_ref().map(|(n, _)| *n))
            .collect::<BTreeSet<_>>();

        let pkg_info = packages_info.lock().unwrap();
        let pkg_cached_deps = match pkg_info.get(pkg_path) {
            Some(d)
                if manifest_hash.is_some()
                    && manifest_hash == d.manifest_hash
                    && deps_hash == d.deps_hash =>
            {
                eprintln!("found cached deps for {:?}", pkg_path);
                Some(AnalyzedPkgInfo {
                    program_deps: d.deps.clone(),
                    symbols_data: Some(d.deps_symbols_data.clone()),
                    program: Some(d.program.clone()),
                    file_hashes: d.file_hashes.clone(),
                })
            }
            _ => construct_pre_compiled_lib(
                src_deps,
                None,
                compiler_flags,
                Some(overlay_fs_root.clone()),
            )
            .ok()
            .and_then(|pprog_and_comments_res| pprog_and_comments_res.ok())
            .map(|libs| {
                eprintln!("created pre-compiled libs for {:?}", pkg_path);
                AnalyzedPkgInfo {
                    program_deps: Arc::new(libs),
                    symbols_data: None,
                    program: None,
                    file_hashes: file_hashes.clone(),
                }
            }),
        };
        if pkg_cached_deps.is_some() {
            // if successful, remove only source deps but keep bytecode deps as they
            // were not used to construct pre-compiled lib in the first place
            dependencies.remove_deps(src_names);
        }
        pkg_cached_deps
    } else {
        None
    };

    let (full_compilation, files_to_compile) = if let Some(chached_info) = &cached_info_opt {
        if chached_info.program.is_some() {
            // we already have cached user program, consider incremental compilation
            match modified_files {
                Some(files) => (false, BTreeSet::from_iter(files)),
                None => (true, BTreeSet::new()),
            }
        } else {
            (true, BTreeSet::new())
        }
    } else {
        (true, BTreeSet::new())
    };

    let mut edition = None;
    let compiled_libs = cached_info_opt
        .clone()
        .map(|deps| deps.program_deps.clone());
    build_plan.compile_with_driver_and_deps(dependencies, &mut std::io::sink(), |compiler| {
        let compiler = compiler.set_ide_mode();
        // extract expansion AST
        let (files, compilation_result) = compiler
            .set_pre_compiled_lib_opt(compiled_libs.clone())
            .set_files_to_compile(if full_compilation {
                None
            } else {
                Some(files_to_compile.clone())
            })
            .run::<PASS_PARSER>()?;
        let compiler = match compilation_result {
            Ok(v) => v,
            Err((_pass, diags)) => {
                let failure = true;
                diagnostics = Some((diags, failure));
                eprintln!("parsed AST compilation failed");
                return Ok((files, vec![]));
            }
        };
        eprintln!("compiled to parsed AST");
        let (compiler, parsed_program) = compiler.into_ast();
        parsed_ast = Some(parsed_program.clone());

        // extract typed AST
        let compilation_result = compiler.at_parser(parsed_program).run::<PASS_TYPING>();
        let compiler = match compilation_result {
            Ok(v) => v,
            Err((_pass, diags)) => {
                let failure = true;
                diagnostics = Some((diags, failure));
                eprintln!("typed AST compilation failed");
                eprintln!("diagnostics: {:#?}", diagnostics);
                return Ok((files, vec![]));
            }
        };
        eprintln!("compiled to typed AST");
        let (compiler, typed_program) = compiler.into_ast();
        typed_ast = Some(typed_program.clone());
        compiler_info = Some(CompilerInfo::from(
            compiler.compilation_env().ide_information().clone(),
        ));
        edition = Some(compiler.compilation_env().edition(Some(root_pkg_name)));

        // compile to CFGIR for accurate diags
        eprintln!("compiling to CFGIR");
        let compilation_result = compiler.at_typing(typed_program).run::<PASS_CFGIR>();
        let compiler = match compilation_result {
            Ok(v) => v,
            Err((_pass, diags)) => {
                let failure = false;
                diagnostics = Some((diags, failure));
                eprintln!("compilation to CFGIR failed");
                return Ok((files, vec![]));
            }
        };
        let failure = false;
        diagnostics = Some((compiler.compilation_env().take_final_diags(), failure));
        eprintln!("compiled to CFGIR");
        Ok((files, vec![]))
    })?;

    let mut ide_diagnostics = lsp_empty_diagnostics(mapped_files.file_name_mapping());
    if let Some((compiler_diagnostics, failure)) = diagnostics {
        let lsp_diagnostics =
            lsp_diagnostics(&compiler_diagnostics.into_codespan_format(), &mapped_files);
        // start with empty diagnostics for all files and replace them with actual diagnostics
        // only for files that have failures/warnings so that diagnostics for all other files
        // (that no longer have failures/warnings) are reset
        ide_diagnostics.extend(lsp_diagnostics);
        if failure {
            // just return diagnostics as we don't have typed AST that we can use to compute
            // symbolication information
            debug_assert!(typed_ast.is_none());
            return Ok((None, ide_diagnostics));
        }
    }

    // uwrap's are safe - this function returns earlier (during diagnostics processing)
    // when failing to produce the ASTs
    let (parsed_program, typed_program_modules) = if full_compilation {
        (parsed_ast.unwrap(), typed_ast.unwrap().modules)
    } else {
        merge_user_programs(
            cached_info_opt.clone(),
            parsed_ast.unwrap(),
            typed_ast.unwrap().modules,
            file_hashes,
            files_to_compile,
        )
    };
    let compiled_pkg_info = CompiledPkgInfo {
        path: pkg_path.into(),
        manifest_hash,
        deps_hash,
        cached_deps: cached_info_opt,
        program: CompiledProgram {
            parsed: parsed_program,
            typed_modules: typed_program_modules,
        },
        mapped_files,
        edition,
        compiler_info,
    };
    Ok((Some(compiled_pkg_info), ide_diagnostics))
}

/// Preprocess parsed and typed programs prior to actual symbols computation.
pub fn compute_symbols_pre_process(
    computation_data: &mut SymbolsComputationData,
    computation_data_deps: &mut SymbolsComputationData,
    compiled_pkg_info: &mut CompiledPkgInfo,
    cursor_info: Option<(&PathBuf, Position)>,
) -> Option<CursorContext> {
    let mut fields_order_info = FieldOrderInfo::new();
    let parsed_program = &compiled_pkg_info.program.parsed;
    let typed_program_modules = &compiled_pkg_info.program.typed_modules;
    pre_process_parsed_program(parsed_program, &mut fields_order_info);

    let mut cursor_context = compute_cursor_context(&compiled_pkg_info.mapped_files, cursor_info);
    pre_process_typed_modules(
        typed_program_modules,
        &fields_order_info,
        &compiled_pkg_info.mapped_files,
        &mut computation_data.mod_outer_defs,
        &mut computation_data.mod_use_defs,
        &mut computation_data.references,
        &mut computation_data.def_info,
        &compiled_pkg_info.edition,
        cursor_context.as_mut(),
    );

    if let Some(cached_deps) = compiled_pkg_info.cached_deps.clone() {
        // we have at least compiled program available
        let (deps_mod_outer_defs, deps_def_info) =
            if let Some(cached_symbols_data) = cached_deps.symbols_data {
                // We have cached results of the dependency symbols computation from the previous run.
                (
                    cached_symbols_data.mod_outer_defs.clone(),
                    cached_symbols_data.def_info.clone(),
                )
            } else {
                // No cached dependency symbols data but we still have cached compilation results.
                // Fill out dependency symbols from compiled package info to cache them at the end of analysis
                pre_process_typed_modules(
                    &cached_deps.program_deps.typing.modules,
                    &FieldOrderInfo::new(),
                    &compiled_pkg_info.mapped_files,
                    &mut computation_data_deps.mod_outer_defs,
                    &mut computation_data_deps.mod_use_defs,
                    &mut computation_data_deps.references,
                    &mut computation_data_deps.def_info,
                    &compiled_pkg_info.edition,
                    None, // Cursor can never be in a compiled library(?)
                );
                (
                    computation_data_deps.mod_outer_defs.clone(),
                    computation_data_deps.def_info.clone(),
                )
            };
        // We need to update definitions for the code being currently processed
        // so that these definitions are available when ASTs for this code are visited
        computation_data.mod_outer_defs.extend(deps_mod_outer_defs);
        computation_data.def_info.extend(deps_def_info);
    }

    cursor_context
}

/// Run parsing analysis for either main program or dependencies
fn run_parsing_analysis(
    computation_data: &mut SymbolsComputationData,
    compiled_pkg_info: &CompiledPkgInfo,
    cursor_context: Option<&mut CursorContext>,
    parsed_program: &P::Program,
) {
    let mut parsing_symbolicator = parsing_analysis::ParsingAnalysisContext {
        mod_outer_defs: &mut computation_data.mod_outer_defs,
        files: &compiled_pkg_info.mapped_files,
        references: &mut computation_data.references,
        def_info: &mut computation_data.def_info,
        use_defs: UseDefMap::new(),
        current_mod_ident_str: None,
        alias_lengths: BTreeMap::new(),
        pkg_addresses: &NamedAddressMap::new(),
        cursor: cursor_context,
    };

    parsing_symbolicator.prog_symbols(
        parsed_program,
        &mut computation_data.mod_use_defs,
        &mut computation_data.mod_to_alias_lengths,
    );
}

/// Process parsed program for symbols computation.
pub fn compute_symbols_parsed_program(
    computation_data: &mut SymbolsComputationData,
    computation_data_deps: &mut SymbolsComputationData,
    compiled_pkg_info: &CompiledPkgInfo,
    mut cursor_context: Option<CursorContext>,
) -> Option<CursorContext> {
    run_parsing_analysis(
        computation_data,
        compiled_pkg_info,
        cursor_context.as_mut(),
        &compiled_pkg_info.program.parsed,
    );
    if let Some(cached_deps) = &compiled_pkg_info.cached_deps {
        // run parsing analysis only if cached symbols computation data
        // is not available to fill out dependency symbols from compiled package info
        // to cache them at the end of analysis
        if cached_deps.symbols_data.is_none() {
            run_parsing_analysis(
                computation_data_deps,
                compiled_pkg_info,
                None,
                &cached_deps.program_deps.parser,
            );
        }
    }
    cursor_context
}

/// Run typing analysis for either main program or dependencies
fn run_typing_analysis(
    mut computation_data: SymbolsComputationData,
    mapped_files: &MappedFiles,
    compiler_info: &mut CompilerInfo,
    typed_program_modules: &UniqueMap<ModuleIdent, ModuleDefinition>,
) -> SymbolsComputationData {
    let mut typing_symbolicator = typing_analysis::TypingAnalysisContext {
        mod_outer_defs: &mut computation_data.mod_outer_defs,
        files: mapped_files,
        references: &mut computation_data.references,
        def_info: &mut computation_data.def_info,
        use_defs: UseDefMap::new(),
        current_mod_ident_str: None,
        alias_lengths: &BTreeMap::new(),
        traverse_only: false,
        compiler_info,
        type_params: BTreeMap::new(),
        expression_scope: OrdMap::new(),
    };

    process_typed_modules(
        typed_program_modules,
        &computation_data.mod_to_alias_lengths,
        &mut typing_symbolicator,
        &mut computation_data.mod_use_defs,
    );
    computation_data
}

// Given use-defs for a the main program or dependencies, update the per-file
// use-def map
fn update_file_use_defs(
    computation_data: &SymbolsComputationData,
    mapped_files: &MappedFiles,
    file_use_defs: &mut FileUseDefs,
) {
    for (module_ident_str, use_defs) in &computation_data.mod_use_defs {
        // unwrap here is safe as all modules in a given program have the module_defs entry
        // in the map
        let module_defs = computation_data
            .mod_outer_defs
            .get(module_ident_str)
            .unwrap();
        let fpath = match mapped_files.file_name_mapping().get(&module_defs.fhash) {
            Some(p) => p.as_path().to_string_lossy().to_string(),
            None => return,
        };

        let fpath_buffer =
            dunce::canonicalize(fpath.clone()).unwrap_or_else(|_| PathBuf::from(fpath.as_str()));

        file_use_defs
            .entry(fpath_buffer)
            .or_default()
            .extend(use_defs.clone().elements());
    }
}

/// Process typed program for symbols computation. Returns:
/// - computed symbols
/// - optional cacheable symbols data (obtained either from cache or recomputed)
/// - compiled user program
pub fn compute_symbols_typed_program(
    computation_data: SymbolsComputationData,
    computation_data_deps: SymbolsComputationData,
    mut compiled_pkg_info: CompiledPkgInfo,
    cursor_context: Option<CursorContext>,
) -> (
    Symbols,
    Option<Arc<SymbolsComputationData>>,
    CompiledProgram,
) {
    // run typing analysis for the main user program
    let compiler_info = &mut compiled_pkg_info.compiler_info.as_mut().unwrap();
    let mapped_files = &compiled_pkg_info.mapped_files;
    let mut computation_data = run_typing_analysis(
        computation_data,
        mapped_files,
        compiler_info,
        &compiled_pkg_info.program.typed_modules,
    );
    let mut file_use_defs = BTreeMap::new();
    update_file_use_defs(&computation_data, mapped_files, &mut file_use_defs);

    let cacheable_symbols_data_opt =
        if let Some(cached_deps) = compiled_pkg_info.cached_deps.clone() {
            // we have at least compiled program available
            let deps_symbols_data = if let Some(cached_symbols_data) = cached_deps.symbols_data {
                // We have cached results of the dependency symbols computation from the previous run.
                cached_symbols_data
            } else {
                // No cached dependency symbols data but we still have cached compilation results.
                // Fill out dependency symbols from compiled package info to cache them at the end of analysis
                let computation_data_deps = run_typing_analysis(
                    computation_data_deps,
                    mapped_files,
                    compiler_info,
                    &cached_deps.program_deps.typing.modules,
                );
                Arc::new(computation_data_deps)
            };
            // create `file_use_defs` map and merge references to produce complete symbols data
            // (mod_outer_defs and def_info have already been merged to facilitate user program
            // analysis)
            update_file_use_defs(&deps_symbols_data, mapped_files, &mut file_use_defs);
            for (def_loc, uses) in &deps_symbols_data.references {
                computation_data
                    .references
                    .entry(*def_loc)
                    .or_default()
                    .extend(uses);
            }
            Some(deps_symbols_data)
        } else {
            None
        };

    let mut file_mods: FileModules = BTreeMap::new();
    for d in computation_data.mod_outer_defs.into_values() {
        let path = compiled_pkg_info.mapped_files.file_path(&d.fhash.clone());
        file_mods.entry(path.to_path_buf()).or_default().insert(d);
    }

    (
        Symbols {
            references: computation_data.references,
            file_use_defs,
            file_mods,
            def_info: computation_data.def_info,
            files: compiled_pkg_info.mapped_files,
            compiler_info: compiled_pkg_info.compiler_info.unwrap(),
            cursor_context,
        },
        cacheable_symbols_data_opt,
        compiled_pkg_info.program,
    )
}

/// Compute symbols for a given package from the parsed and typed ASTs,
/// as well as other auxiliary data provided in `compiled_pkg_info`.
pub fn compute_symbols(
    packages_info: Arc<Mutex<BTreeMap<PathBuf, PrecomputedPkgInfo>>>,
    mut compiled_pkg_info: CompiledPkgInfo,
    cursor_info: Option<(&PathBuf, Position)>,
) -> Symbols {
    let pkg_path = compiled_pkg_info.path.clone();
    let manifest_hash = compiled_pkg_info.manifest_hash;
    let cached_dep_opt = compiled_pkg_info.cached_deps.clone();
    let deps_hash = compiled_pkg_info.deps_hash.clone();
    let file_hashes = compiled_pkg_info
        .mapped_files
        .file_name_mapping()
        .iter()
        .map(|(fhash, fpath)| (fpath.clone(), *fhash))
        .collect::<BTreeMap<_, _>>();
    let mut symbols_computation_data = SymbolsComputationData::new();
    let mut symbols_computation_data_deps = SymbolsComputationData::new();
    let cursor_context = compute_symbols_pre_process(
        &mut symbols_computation_data,
        &mut symbols_computation_data_deps,
        &mut compiled_pkg_info,
        cursor_info,
    );
    let cursor_context = compute_symbols_parsed_program(
        &mut symbols_computation_data,
        &mut symbols_computation_data_deps,
        &compiled_pkg_info,
        cursor_context,
    );

    let (symbols, cacheable_symbols_data_opt, program) = compute_symbols_typed_program(
        symbols_computation_data,
        symbols_computation_data_deps,
        compiled_pkg_info,
        cursor_context,
    );

    let mut pkg_deps = packages_info.lock().unwrap();

    if let Some(cached_deps) = cached_dep_opt {
        // we have at least compiled program available, either already cached
        // or created for the purpose of this analysis
        if let Some(deps_symbols_data) = cacheable_symbols_data_opt {
            // dependencies may have changed or not, but we still need to update the cache
            // with new file hashes and user program info
            eprintln!("caching pre-compiled program and pre-computed symbols");
            pkg_deps.insert(
                pkg_path,
                PrecomputedPkgInfo {
                    manifest_hash,
                    deps_hash,
                    deps: cached_deps.program_deps.clone(),
                    deps_symbols_data,
                    program: Arc::new(program),
                    file_hashes: Arc::new(file_hashes),
                },
            );
        }
    }
    symbols
}

/// Main driver to get symbols for the whole package. Returned symbols is an option as only the
/// correctly computed symbols should be a replacement for the old set - if symbols are not
/// actually (re)computed and the diagnostics are returned, the old symbolic information should
/// be retained even if it's getting out-of-date.
///
/// Takes `modified_files` as an argument to indicate if we can retain (portion of) the cached
/// user code. If `modified_files` is `None`, we can't retain any cached user code (need to recompute)
/// everything. If `modified_files` is `Some`, we can retain cached user code for all Move files other than
/// the ones in `modified_files` (if `modified_paths` contains a path not representing
/// a Move file but rather a directory, then we conservatively do not re-use any cached info).
pub fn get_symbols(
    packages_info: Arc<Mutex<BTreeMap<PathBuf, PrecomputedPkgInfo>>>,
    ide_files_root: VfsPath,
    pkg_path: &Path,
    modified_files: Option<Vec<PathBuf>>,
    lint: LintLevel,
    cursor_info: Option<(&PathBuf, Position)>,
) -> Result<(Option<Symbols>, BTreeMap<PathBuf, Vec<Diagnostic>>)> {
    let compilation_start = Instant::now();
    let (compiled_pkg_info_opt, ide_diagnostics) = get_compiled_pkg(
        packages_info.clone(),
        ide_files_root,
        pkg_path,
        modified_files,
        lint,
    )?;
    eprintln!("compilation complete in: {:?}", compilation_start.elapsed());
    let Some(compiled_pkg_info) = compiled_pkg_info_opt else {
        return Ok((None, ide_diagnostics));
    };
    let analysis_start = Instant::now();
    let symbols = compute_symbols(packages_info, compiled_pkg_info, cursor_info);
    eprintln!("analysis complete in {:?}", analysis_start.elapsed());
    eprintln!("get_symbols load complete");

    Ok((Some(symbols), ide_diagnostics))
}

fn compute_cursor_context(
    mapped_files: &MappedFiles,
    cursor_info: Option<(&PathBuf, Position)>,
) -> Option<CursorContext> {
    let (path, pos) = cursor_info?;
    let file_hash = mapped_files.file_hash(path)?;
    let loc = lsp_position_to_loc(mapped_files, file_hash, &pos)?;
    eprintln!("computed cursor loc");
    Some(CursorContext::new(loc))
}

/// Pre-process parsed program to get initial info before AST traversals
fn pre_process_parsed_program(prog: &P::Program, fields_order_info: &mut FieldOrderInfo) {
    prog.source_definitions.iter().for_each(|pkg_def| {
        pre_process_parsed_pkg(pkg_def, &prog.named_address_maps, fields_order_info);
    });
    prog.lib_definitions.iter().for_each(|pkg_def| {
        pre_process_parsed_pkg(pkg_def, &prog.named_address_maps, fields_order_info);
    });
}

/// Pre-process parsed package to get initial info before AST traversals
fn pre_process_parsed_pkg(
    pkg_def: &P::PackageDefinition,
    named_address_maps: &NamedAddressMaps,
    fields_order_info: &mut FieldOrderInfo,
) {
    if let P::Definition::Module(mod_def) = &pkg_def.def {
        for member in &mod_def.members {
            let pkg_addresses = named_address_maps.get(pkg_def.named_address_map);
            let Some(mod_ident_str) = parsing_mod_def_to_map_key(pkg_addresses, mod_def) else {
                continue;
            };
            if let P::ModuleMember::Struct(sdef) = member {
                if let P::StructFields::Named(fields) = &sdef.fields {
                    let indexed_fields = fields
                        .iter()
                        .enumerate()
                        .map(|(i, (_, f, _))| (f.value(), i))
                        .collect::<BTreeMap<_, _>>();
                    fields_order_info
                        .structs
                        .entry(mod_ident_str.clone())
                        .or_default()
                        .entry(sdef.name.value())
                        .or_default()
                        .extend(indexed_fields);
                }
            }
            if let P::ModuleMember::Enum(edef) = member {
                for vdef in &edef.variants {
                    if let P::VariantFields::Named(fields) = &vdef.fields {
                        let indexed_fields = fields
                            .iter()
                            .enumerate()
                            .map(|(i, (_, f, _))| (f.value(), i))
                            .collect::<BTreeMap<_, _>>();
                        fields_order_info
                            .variants
                            .entry(mod_ident_str.clone())
                            .or_default()
                            .entry(edef.name.value())
                            .or_default()
                            .entry(vdef.name.value())
                            .or_default()
                            .extend(indexed_fields);
                    }
                }
            }
        }
    }
}

fn pre_process_typed_modules(
    typed_modules: &UniqueMap<ModuleIdent, ModuleDefinition>,
    fields_order_info: &FieldOrderInfo,
    files: &MappedFiles,
    mod_outer_defs: &mut BTreeMap<String, ModuleDefs>,
    mod_use_defs: &mut BTreeMap<String, UseDefMap>,
    references: &mut References,
    def_info: &mut DefMap,
    edition: &Option<Edition>,
    mut cursor_context: Option<&mut CursorContext>,
) {
    for (pos, module_ident, module_def) in typed_modules {
        // If the cursor is in this module, mark that down.
        if let Some(cursor) = &mut cursor_context {
            if module_def.loc.contains(&cursor.loc) {
                cursor.module = Some(sp(pos, *module_ident));
            }
        };

        let mod_ident_str = expansion_mod_ident_to_map_key(module_ident);
        let (defs, symbols) = get_mod_outer_defs(
            &pos,
            &sp(pos, *module_ident),
            mod_ident_str.clone(),
            module_def,
            fields_order_info,
            files,
            references,
            def_info,
            edition,
        );
        mod_outer_defs.insert(mod_ident_str.clone(), defs);
        mod_use_defs.insert(mod_ident_str, symbols);
    }
}

fn process_typed_modules<'a>(
    typed_modules: &UniqueMap<ModuleIdent, ModuleDefinition>,
    mod_to_alias_lengths: &'a BTreeMap<String, BTreeMap<Position, usize>>,
    typing_symbolicator: &mut typing_analysis::TypingAnalysisContext<'a>,
    mod_use_defs: &mut BTreeMap<String, UseDefMap>,
) {
    for (module_ident, module_def) in typed_modules.key_cloned_iter() {
        let mod_ident_str = expansion_mod_ident_to_map_key(&module_ident.value);
        typing_symbolicator.use_defs = mod_use_defs.remove(&mod_ident_str).unwrap();
        typing_symbolicator.alias_lengths = mod_to_alias_lengths.get(&mod_ident_str).unwrap();
        typing_symbolicator.visit_module(module_ident, module_def);

        let use_defs = std::mem::replace(&mut typing_symbolicator.use_defs, UseDefMap::new());
        mod_use_defs.insert(mod_ident_str, use_defs);
    }
}

fn compute_mapped_files(
    resolved_graph: &ResolvedGraph,
    overlay_fs: VfsPath,
) -> (MappedFiles, String) {
    let mut mapped_files: MappedFiles = MappedFiles::empty();
    let mut hasher = Sha256::new();
    for rpkg in resolved_graph.package_table.values() {
        for f in rpkg.get_sources(&resolved_graph.build_options).unwrap() {
            let is_dep = rpkg.package_path != resolved_graph.graph.root_path;
            // dunce does a better job of canonicalization on Windows
            let fname = dunce::canonicalize(f.as_str())
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| f.to_string());
            let mut contents = String::new();
            // there is a fair number of unwraps here but if we can't read the files
            // that by all accounts should be in the file system, then there is not much
            // we can do so it's better to fail so that we can investigate
            let vfs_file_path = overlay_fs.join(fname.as_str()).unwrap();
            let mut vfs_file = vfs_file_path.open_file().unwrap();
            let _ = vfs_file.read_to_string(&mut contents);
            let fhash = FileHash::new(&contents);
            if is_dep {
                hasher.update(fhash.0);
            }
            // write to top layer of the overlay file system so that the content
            // is immutable for the duration of compilation and symbolication
            let _ = vfs_file_path.parent().create_dir_all();
            let mut vfs_file = vfs_file_path.create_file().unwrap();
            let _ = vfs_file.write_all(contents.as_bytes());
            mapped_files.add(fhash, fname.into(), Arc::from(contents.into_boxed_str()));
        }
    }
    (mapped_files, format!("{:X}", hasher.finalize()))
}

/// Produces module ident string of the form pkg::module to be used as a map key
/// It's important that these are consistent between parsing AST and typed AST.
pub fn expansion_mod_ident_to_map_key(mod_ident: &E::ModuleIdent_) -> String {
    use E::Address as A;
    match mod_ident.address {
        A::Numerical {
            name,
            value,
            name_conflict: _,
        } => {
            if let Some(n) = name {
                format!("({n}={value})::{}", mod_ident.module).to_string()
            } else {
                format!("{value}::{}", mod_ident.module).to_string()
            }
        }
        A::NamedUnassigned(n) => format!("{n}::{}", mod_ident.module).to_string(),
    }
}

/// Converts parsing AST's `LeadingNameAccess` to expansion AST's `Address` (similarly to
/// expansion::translate::top_level_address but disregarding the name portion of `Address` as we
/// only care about actual address here if it's available). We need this to be able to reliably
/// compare parsing AST's module identifier with expansion/typing AST's module identifier, even in
/// presence of module renaming (i.e., we cannot rely on module names if addresses are available).
pub fn parsed_address(ln: P::LeadingNameAccess, pkg_addresses: &NamedAddressMap) -> E::Address {
    let sp!(loc, ln_) = ln;
    match ln_ {
        P::LeadingNameAccess_::AnonymousAddress(bytes) => E::Address::anonymous(loc, bytes),
        P::LeadingNameAccess_::GlobalAddress(name) => E::Address::NamedUnassigned(name),
        P::LeadingNameAccess_::Name(name) => match pkg_addresses.get(&name.value).copied() {
            // set `name_conflict` to `true` to force displaying (addr==pkg_name) so that the string
            // representing map key is consistent with what's generated for expansion ModuleIdent in
            // `expansion_mod_ident_to_map_key`
            Some(addr) => E::Address::Numerical {
                name: Some(name),
                value: sp(loc, addr),
                name_conflict: true,
            },
            None => E::Address::NamedUnassigned(name),
        },
    }
}

/// Produces module ident string of the form pkg::module to be used as a map key.
/// It's important that these are consistent between parsing AST and typed AST.
pub fn parsing_leading_and_mod_names_to_map_key(
    pkg_addresses: &NamedAddressMap,
    ln: P::LeadingNameAccess,
    name: P::ModuleName,
) -> String {
    let parsed_addr = parsed_address(ln, pkg_addresses);
    format!("{}::{}", parsed_addr, name).to_string()
}

/// Produces module ident string of the form pkg::module to be used as a map key.
/// It's important that these are consistent between parsing AST and typed AST.
pub fn parsing_mod_def_to_map_key(
    pkg_addresses: &NamedAddressMap,
    mod_def: &P::ModuleDefinition,
) -> Option<String> {
    // we assume that modules are declared using the PkgName::ModName pattern (which seems to be the
    // standard practice) and while Move allows other ways of defining modules (i.e., with address
    // preceding a sequence of modules), this method is now deprecated.
    //
    // TODO: make this function simply return String when the other way of defining modules is
    // removed
    mod_def
        .address
        .map(|a| parsing_leading_and_mod_names_to_map_key(pkg_addresses, a, mod_def.name))
}

/// Get empty symbols
pub fn empty_symbols() -> Symbols {
    Symbols {
        file_use_defs: BTreeMap::new(),
        references: BTreeMap::new(),
        file_mods: BTreeMap::new(),
        def_info: BTreeMap::new(),
        files: MappedFiles::empty(),
        compiler_info: CompilerInfo::new(),
        cursor_context: None,
    }
}

fn field_defs_and_types(
    datatype_name: Symbol,
    fields: &E::Fields<(DocComment, Type)>,
    fields_order_opt: Option<&BTreeMap<Symbol, usize>>,
    mod_ident: &ModuleIdent,
    def_info: &mut DefMap,
) -> (Vec<FieldDef>, Vec<Type>) {
    let mut field_defs = vec![];
    let mut field_types = vec![];
    let mut ordered_fields = fields
        .iter()
        .map(|(floc, fname, (_, (fdoc, ftype)))| (floc, fdoc, fname, ftype))
        .collect::<Vec<_>>();
    // sort fields by order if available for correct auto-completion
    if let Some(fields_order) = fields_order_opt {
        ordered_fields.sort_by_key(|(_, _, fname, _)| fields_order.get(fname).copied());
    }
    for (floc, fdoc, fname, ftype) in ordered_fields {
        field_defs.push(FieldDef {
            name: *fname,
            loc: floc,
        });
        let doc_string = fdoc.comment().map(|d| d.value.to_owned());
        def_info.insert(
            floc,
            DefInfo::Field(
                mod_ident.value,
                datatype_name,
                *fname,
                ftype.clone(),
                doc_string,
            ),
        );
        field_types.push(ftype.clone());
    }
    (field_defs, field_types)
}

fn datatype_type_params(data_tparams: &[DatatypeTypeParameter]) -> Vec<(Type, /* phantom */ bool)> {
    data_tparams
        .iter()
        .map(|t| {
            (
                sp(
                    t.param.user_specified_name.loc,
                    Type_::Param(t.param.clone()),
                ),
                t.is_phantom,
            )
        })
        .collect()
}

/// Some functions defined in a module need to be ignored.
pub fn ignored_function(name: Symbol) -> bool {
    // In test mode (that's how IDE compiles Move source files), the compiler inserts an dummy
    // function preventing publishing of modules compiled in test mode. We need to ignore its
    // definition to avoid spurious on-hover display of this function's info whe hovering close to
    // `module` keyword.
    name == UNIT_TEST_POISON_FUN_NAME
}

/// Get symbols for outer definitions in the module (functions, structs, and consts)
fn get_mod_outer_defs(
    loc: &Loc,
    mod_ident: &ModuleIdent,
    mod_ident_str: String,
    mod_def: &ModuleDefinition,
    fields_order_info: &FieldOrderInfo,
    files: &MappedFiles,
    references: &mut References,
    def_info: &mut DefMap,
    edition: &Option<Edition>,
) -> (ModuleDefs, UseDefMap) {
    let mut structs = BTreeMap::new();
    let mut enums = BTreeMap::new();
    let mut constants = BTreeMap::new();
    let mut functions = BTreeMap::new();

    let fhash = loc.file_hash();
    let mut positional = false;
    for (name_loc, name, def) in &mod_def.structs {
        // process struct fields first
        let mut field_defs = vec![];
        let mut field_types = vec![];
        if let StructFields::Defined(pos_fields, fields) = &def.fields {
            positional = *pos_fields;
            let fields_order_opt = fields_order_info
                .structs
                .get(&mod_ident_str)
                .and_then(|s| s.get(name));
            (field_defs, field_types) =
                field_defs_and_types(*name, fields, fields_order_opt, mod_ident, def_info);
        };

        // process the struct itself
        let field_names = field_defs.iter().map(|f| sp(f.loc, f.name)).collect();
        structs.insert(
            *name,
            MemberDef {
                name_loc,
                info: MemberDefInfo::Struct {
                    field_defs,
                    positional,
                },
            },
        );
        let pub_struct = edition
            .map(|e| e.supports(FeatureGate::PositionalFields))
            .unwrap_or(false);
        let visibility = if pub_struct {
            // fake location OK as this is for display purposes only
            Visibility::Public(Loc::invalid())
        } else {
            Visibility::Internal
        };
        let doc_string = def.doc.comment().map(|d| d.value.to_owned());
        def_info.insert(
            name_loc,
            DefInfo::Struct(
                mod_ident.value,
                *name,
                visibility,
                datatype_type_params(&def.type_parameters),
                def.abilities.clone(),
                field_names,
                field_types,
                doc_string,
            ),
        );
    }

    for (name_loc, name, def) in &mod_def.enums {
        // process variants
        let mut variants_info = BTreeMap::new();
        let mut def_info_variants = vec![];
        for (vname_loc, vname, vdef) in &def.variants {
            let (field_defs, field_types, positional) = match &vdef.fields {
                VariantFields::Defined(pos_fields, fields) => {
                    let fields_order_opt = fields_order_info
                        .variants
                        .get(&mod_ident_str)
                        .and_then(|v| v.get(name))
                        .and_then(|v| v.get(vname));
                    let (defs, types) =
                        field_defs_and_types(*name, fields, fields_order_opt, mod_ident, def_info);
                    (defs, types, *pos_fields)
                }
                VariantFields::Empty => (vec![], vec![], false),
            };
            let field_names = field_defs.iter().map(|f| sp(f.loc, f.name)).collect();
            def_info_variants.push(VariantInfo {
                name: sp(vname_loc, *vname),
                empty: field_defs.is_empty(),
                positional,
            });
            variants_info.insert(*vname, (vname_loc, field_defs, positional));

            let vdoc_string = def.doc.comment().map(|d| d.value.to_owned());
            def_info.insert(
                vname_loc,
                DefInfo::Variant(
                    mod_ident.value,
                    *name,
                    *vname,
                    positional,
                    field_names,
                    field_types,
                    vdoc_string,
                ),
            );
        }
        // process the enum itself
        enums.insert(
            *name,
            MemberDef {
                name_loc,
                info: MemberDefInfo::Enum { variants_info },
            },
        );
        let enum_doc_string = def.doc.comment().map(|d| d.value.to_owned());
        def_info.insert(
            name_loc,
            DefInfo::Enum(
                mod_ident.value,
                *name,
                Visibility::Public(Loc::invalid()),
                datatype_type_params(&def.type_parameters),
                def.abilities.clone(),
                def_info_variants,
                enum_doc_string,
            ),
        );
    }

    for (name_loc, name, c) in &mod_def.constants {
        constants.insert(
            *name,
            MemberDef {
                name_loc,
                info: MemberDefInfo::Const,
            },
        );
        let doc_string = c.doc.comment().map(|d| d.value.to_owned());
        def_info.insert(
            name_loc,
            DefInfo::Const(
                mod_ident.value,
                *name,
                c.signature.clone(),
                const_val_to_ide_string(&c.value),
                doc_string,
            ),
        );
    }

    for (name_loc, name, fun) in &mod_def.functions {
        if ignored_function(*name) {
            continue;
        }
        let fun_type = if fun.entry.is_some() {
            FunType::Entry
        } else if fun.macro_.is_some() {
            FunType::Macro
        } else {
            FunType::Regular
        };
        let doc_string = fun.doc.comment().map(|d| d.value.to_owned());
        let fun_info = DefInfo::Function(
            mod_ident.value,
            fun.visibility,
            fun_type,
            *name,
            fun.signature
                .type_parameters
                .iter()
                .map(|t| (sp(t.user_specified_name.loc, Type_::Param(t.clone()))))
                .collect(),
            fun.signature
                .parameters
                .iter()
                .map(|(_, n, _)| sp(n.loc, n.value.name))
                .collect(),
            fun.signature
                .parameters
                .iter()
                .map(|(_, _, t)| t.clone())
                .collect(),
            fun.signature.return_type.clone(),
            doc_string,
        );
        functions.insert(
            *name,
            MemberDef {
                name_loc,
                info: MemberDefInfo::Fun {
                    attrs: fun
                        .attributes
                        .clone()
                        .iter()
                        .map(|(_loc, name, _attr)| name.to_string())
                        .collect(),
                },
            },
        );
        def_info.insert(name_loc, fun_info);
    }

    let mut use_def_map = UseDefMap::new();

    let ident = mod_ident.value;
    let doc_string = mod_def.doc.comment().map(|d| d.value.to_owned());
    let mod_defs = ModuleDefs {
        fhash,
        ident,
        name_loc: *loc,
        structs,
        enums,
        constants,
        functions,
        untyped_defs: BTreeSet::new(),
        call_infos: BTreeMap::new(),
    };

    // insert use of the module name in the definition itself
    let mod_name = ident.module;
    if let Some(mod_name_start) = loc_start_to_lsp_position_opt(files, &mod_name.loc()) {
        use_def_map.insert(
            mod_name_start.line,
            UseDef::new(
                references,
                &BTreeMap::new(),
                mod_name.loc().file_hash(),
                mod_name_start,
                mod_defs.name_loc,
                &mod_name.value(),
                None,
            ),
        );
        def_info.insert(
            mod_defs.name_loc,
            DefInfo::Module(mod_ident_to_ide_string(&ident, None, false), doc_string),
        );
    }

    (mod_defs, use_def_map)
}

/// Add use of a function, method, struct or enum identifier
pub fn add_member_use_def(
    member_def_name: &Symbol, // may be different from use_name for methods
    files: &MappedFiles,
    mod_defs: &ModuleDefs,
    use_name: &Symbol,
    use_loc: &Loc,
    references: &mut References,
    def_info: &DefMap,
    use_defs: &mut UseDefMap,
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
        use_defs.insert(name_start.line, ud.clone());
        return Some(ud);
    }
    None
}

pub fn def_info_doc_string(def_info: &DefInfo) -> Option<String> {
    match def_info {
        DefInfo::Type(_) => None,
        DefInfo::Function(.., s) => s.clone(),
        DefInfo::Struct(.., s) => s.clone(),
        DefInfo::Enum(.., s) => s.clone(),
        DefInfo::Variant(.., s) => s.clone(),
        DefInfo::Field(.., s) => s.clone(),
        DefInfo::Local(..) => None,
        DefInfo::Const(.., s) => s.clone(),
        DefInfo::Module(_, s) => s.clone(),
    }
}

pub fn type_def_loc(
    mod_outer_defs: &BTreeMap<String, ModuleDefs>,
    sp!(_, t): &Type,
) -> Option<Loc> {
    match t {
        Type_::Ref(_, r) => type_def_loc(mod_outer_defs, r),
        Type_::Apply(_, sp!(_, TypeName_::ModuleType(sp!(_, mod_ident), struct_name)), _) => {
            let mod_ident_str = expansion_mod_ident_to_map_key(mod_ident);
            mod_outer_defs
                .get(&mod_ident_str)
                .and_then(|mod_defs| find_datatype(mod_defs, &struct_name.value()))
        }
        _ => None,
    }
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

/// Handles go-to-def request of the language server
pub fn on_go_to_def_request(context: &Context, request: &Request) {
    let symbols_map = &context.symbols.lock().unwrap();
    let parameters = serde_json::from_value::<GotoDefinitionParams>(request.params.clone())
        .expect("could not deserialize go-to-def request");

    let fpath = parameters
        .text_document_position_params
        .text_document
        .uri
        .to_file_path()
        .unwrap();
    let loc = parameters.text_document_position_params.position;
    let line = loc.line;
    let col = loc.character;

    on_use_request(
        context,
        symbols_map,
        &fpath,
        line,
        col,
        request.id.clone(),
        |u, symbols| {
            let loc = def_ide_location(&u.def_loc, symbols);
            Some(serde_json::to_value(loc).unwrap())
        },
    );
}

pub fn def_ide_location(def_loc: &Loc, symbols: &Symbols) -> Location {
    // TODO: Do we need beginning and end of the definition? Does not seem to make a
    // difference from the IDE perspective as the cursor goes to the beginning anyway (at
    // least in VSCode).
    let span = symbols.files.position_opt(def_loc).unwrap();
    let range = Range {
        start: span.start.into(),
        end: span.end.into(),
    };
    let path = symbols.files.file_path(&def_loc.file_hash());
    Location {
        uri: Url::from_file_path(path).unwrap(),
        range,
    }
}

/// Handles go-to-type-def request of the language server
pub fn on_go_to_type_def_request(context: &Context, request: &Request) {
    let symbols_map = &context.symbols.lock().unwrap();
    let parameters = serde_json::from_value::<GotoTypeDefinitionParams>(request.params.clone())
        .expect("could not deserialize go-to-type-def request");

    let fpath = parameters
        .text_document_position_params
        .text_document
        .uri
        .to_file_path()
        .unwrap();
    let loc = parameters.text_document_position_params.position;
    let line = loc.line;
    let col = loc.character;

    on_use_request(
        context,
        symbols_map,
        &fpath,
        line,
        col,
        request.id.clone(),
        |u, symbols| {
            u.type_def_loc.map(|def_loc| {
                let loc = def_ide_location(&def_loc, symbols);
                serde_json::to_value(loc).unwrap()
            })
        },
    );
}

/// Handles go-to-references request of the language server
pub fn on_references_request(context: &Context, request: &Request) {
    let symbols_map = &context.symbols.lock().unwrap();
    let parameters = serde_json::from_value::<ReferenceParams>(request.params.clone())
        .expect("could not deserialize references request");

    let fpath = parameters
        .text_document_position
        .text_document
        .uri
        .to_file_path()
        .unwrap();
    let loc = parameters.text_document_position.position;
    let line = loc.line;
    let col = loc.character;
    let include_decl = parameters.context.include_declaration;

    on_use_request(
        context,
        symbols_map,
        &fpath,
        line,
        col,
        request.id.clone(),
        |u, symbols| {
            let def_posn = symbols.files.file_start_position_opt(&u.def_loc)?;
            symbols
                .references
                .get(&u.def_loc)
                .map(|s| {
                    let mut locs = vec![];

                    for ref_loc in s {
                        if include_decl
                            || !(Into::<Position>::into(def_posn.position) == ref_loc.start
                                && def_posn.file_hash == ref_loc.fhash)
                        {
                            let end_pos = Position {
                                line: ref_loc.start.line,
                                character: ref_loc.col_end,
                            };
                            let range = Range {
                                start: ref_loc.start,
                                end: end_pos,
                            };
                            let path = symbols.files.file_path(&ref_loc.fhash);
                            locs.push(Location {
                                uri: Url::from_file_path(path).unwrap(),
                                range,
                            });
                        }
                    }
                    locs
                })
                .map(|locs| serde_json::to_value(locs).unwrap())
        },
    );
}

/// Helper function that take a DefInfo, checks if it represents
/// a enum arm variable defintion, and if need be converts it
/// to the one that represents an enum guard variable (which
/// has immutable reference type regarldes of arm variable definition
/// type).
pub fn maybe_convert_for_guard(
    def_info: &DefInfo,
    use_fpath: &Path,
    position: &Position,
    symbols: &Symbols,
) -> Option<DefInfo> {
    let DefInfo::Local(name, ty, is_let, is_mut, guard_loc) = def_info else {
        return None;
    };
    let gloc = (*guard_loc)?;
    let fhash = symbols.file_hash(use_fpath)?;
    let loc = lsp_position_to_loc(&symbols.files, fhash, position)?;
    if symbols.compiler_info.inside_guard(fhash, &loc, &gloc) {
        let new_ty = sp(
            ty.loc,
            Type_::Ref(false, Box::new(sp(ty.loc, ty.value.base_type_()))),
        );
        return Some(DefInfo::Local(*name, new_ty, *is_let, *is_mut, *guard_loc));
    }
    None
}

/// Handles hover request of the language server
pub fn on_hover_request(context: &Context, request: &Request) {
    let symbols_map = &context.symbols.lock().unwrap();
    let parameters = serde_json::from_value::<HoverParams>(request.params.clone())
        .expect("could not deserialize hover request");

    let fpath = parameters
        .text_document_position_params
        .text_document
        .uri
        .to_file_path()
        .unwrap();
    let loc = parameters.text_document_position_params.position;
    let line = loc.line;
    let col = loc.character;

    on_use_request(
        context,
        symbols_map,
        &fpath,
        line,
        col,
        request.id.clone(),
        |u, symbols| {
            let Some(info) = symbols.def_info.get(&u.def_loc) else {
                return Some(serde_json::to_value(Option::<lsp_types::Location>::None).unwrap());
            };
            let contents =
                if let Some(guard_info) = maybe_convert_for_guard(info, &fpath, &loc, symbols) {
                    HoverContents::Markup(on_hover_markup(&guard_info))
                } else {
                    HoverContents::Markup(on_hover_markup(info))
                };
            let range = None;
            Some(serde_json::to_value(Hover { contents, range }).unwrap())
        },
    );
}

pub fn on_hover_markup(info: &DefInfo) -> MarkupContent {
    // use rust for highlighting in Markdown until there is support for Move
    let value = if let Some(s) = &def_info_doc_string(info) {
        format!("```rust\n{}\n```\n{}", info, s)
    } else {
        format!("```rust\n{}\n```", info)
    };
    MarkupContent {
        kind: MarkupKind::Markdown,
        value,
    }
}

/// Helper function to handle language server queries related to identifier uses
pub fn on_use_request(
    context: &Context,
    symbols_map: &BTreeMap<PathBuf, Symbols>,
    use_fpath: &PathBuf,
    use_line: u32,
    use_col: u32,
    id: RequestId,
    use_def_action: impl Fn(&UseDef, &Symbols) -> Option<serde_json::Value>,
) {
    let mut result = None;

    if let Some(symbols) =
        SymbolicatorRunner::root_dir(use_fpath).and_then(|pkg_path| symbols_map.get(&pkg_path))
    {
        if let Some(mod_symbols) = symbols.file_use_defs.get(use_fpath) {
            if let Some(uses) = mod_symbols.get(use_line) {
                for u in uses {
                    if use_col >= u.col_start && use_col <= u.col_end {
                        result = use_def_action(&u, symbols);
                    }
                }
            }
        }
    }
    eprintln!(
        "about to send use response (symbols found: {})",
        result.is_some()
    );

    if result.is_none() {
        result = Some(serde_json::to_value(Option::<lsp_types::Location>::None).unwrap());
    }

    // unwrap will succeed based on the logic above which the compiler is unable to figure out
    // without using Option
    let response = lsp_server::Response::new_ok(id, result.unwrap());
    if let Err(err) = context
        .connection
        .sender
        .send(lsp_server::Message::Response(response))
    {
        eprintln!("could not send use response: {:?}", err);
    }
}

/// Handles document symbol request of the language server
#[allow(deprecated)]
pub fn on_document_symbol_request(context: &Context, request: &Request) {
    let symbols_map = &context.symbols.lock().unwrap();
    let parameters = serde_json::from_value::<DocumentSymbolParams>(request.params.clone())
        .expect("could not deserialize document symbol request");

    let fpath = parameters.text_document.uri.to_file_path().unwrap();
    eprintln!("on_document_symbol_request: {:?}", fpath);

    let mut defs: Vec<DocumentSymbol> = vec![];
    if let Some(symbols) =
        SymbolicatorRunner::root_dir(&fpath).and_then(|pkg_path| symbols_map.get(&pkg_path))
    {
        let empty_mods: BTreeSet<ModuleDefs> = BTreeSet::new();
        let mods = symbols.file_mods.get(&fpath).unwrap_or(&empty_mods);

        for mod_def in mods {
            let name = mod_def.ident.module.clone().to_string();
            let detail = Some(mod_def.ident.clone().to_string());
            let kind = SymbolKind::MODULE;
            let Some(range) = symbols.files.lsp_range_opt(&mod_def.name_loc) else {
                continue;
            };

            let mut children = vec![];

            // handle constants
            for (sym, const_def) in &mod_def.constants {
                let Some(const_range) = symbols.files.lsp_range_opt(&const_def.name_loc) else {
                    continue;
                };
                children.push(DocumentSymbol {
                    name: sym.clone().to_string(),
                    detail: None,
                    kind: SymbolKind::CONSTANT,
                    range: const_range,
                    selection_range: const_range,
                    children: None,
                    tags: Some(vec![]),
                    deprecated: Some(false),
                });
            }

            // handle structs
            for (sym, struct_def) in &mod_def.structs {
                let Some(struct_range) = symbols.files.lsp_range_opt(&struct_def.name_loc) else {
                    continue;
                };

                let fields = struct_field_symbols(struct_def, symbols);
                children.push(DocumentSymbol {
                    name: sym.clone().to_string(),
                    detail: None,
                    kind: SymbolKind::STRUCT,
                    range: struct_range,
                    selection_range: struct_range,
                    children: Some(fields),
                    tags: Some(vec![]),
                    deprecated: Some(false),
                });
            }

            // handle enums
            for (sym, enum_def) in &mod_def.enums {
                let Some(enum_range) = symbols.files.lsp_range_opt(&enum_def.name_loc) else {
                    continue;
                };

                let variants = enum_variant_symbols(enum_def, symbols);
                children.push(DocumentSymbol {
                    name: sym.clone().to_string(),
                    detail: None,
                    kind: SymbolKind::ENUM,
                    range: enum_range,
                    selection_range: enum_range,
                    children: Some(variants),
                    tags: Some(vec![]),
                    deprecated: Some(false),
                });
            }

            // handle functions
            for (sym, func_def) in &mod_def.functions {
                let MemberDefInfo::Fun { attrs } = &func_def.info else {
                    continue;
                };
                let Some(func_range) = symbols.files.lsp_range_opt(&func_def.name_loc) else {
                    continue;
                };

                let mut detail = None;
                if !attrs.is_empty() {
                    detail = Some(format!("{:?}", attrs));
                }

                children.push(DocumentSymbol {
                    name: sym.clone().to_string(),
                    detail,
                    kind: SymbolKind::FUNCTION,
                    range: func_range,
                    selection_range: func_range,
                    children: None,
                    tags: Some(vec![]),
                    deprecated: Some(false),
                });
            }

            defs.push(DocumentSymbol {
                name,
                detail,
                kind,
                range,
                selection_range: range,
                children: Some(children),
                tags: Some(vec![]),
                deprecated: Some(false),
            });
        }
    }
    // unwrap will succeed based on the logic above which the compiler is unable to figure out
    let response = lsp_server::Response::new_ok(request.id.clone(), defs);
    if let Err(err) = context
        .connection
        .sender
        .send(lsp_server::Message::Response(response))
    {
        eprintln!("could not send use response: {:?}", err);
    }
}

/// Helper function to generate struct field symbols
#[allow(deprecated)]
fn struct_field_symbols(struct_def: &MemberDef, symbols: &Symbols) -> Vec<DocumentSymbol> {
    let mut fields: Vec<DocumentSymbol> = vec![];
    if let MemberDefInfo::Struct {
        field_defs,
        positional: _,
    } = &struct_def.info
    {
        for field_def in field_defs {
            let Some(field_range) = symbols.files.lsp_range_opt(&field_def.loc) else {
                continue;
            };

            fields.push(DocumentSymbol {
                name: field_def.name.clone().to_string(),
                detail: None,
                kind: SymbolKind::FIELD,
                range: field_range,
                selection_range: field_range,
                children: None,
                tags: Some(vec![]),
                deprecated: Some(false),
            });
        }
    }
    fields
}

/// Helper function to generate enum variant symbols
#[allow(deprecated)]
fn enum_variant_symbols(enum_def: &MemberDef, symbols: &Symbols) -> Vec<DocumentSymbol> {
    let mut variants: Vec<DocumentSymbol> = vec![];
    if let MemberDefInfo::Enum { variants_info } = &enum_def.info {
        for (name, (loc, _, _)) in variants_info {
            let Some(variant_range) = symbols.files.lsp_range_opt(loc) else {
                continue;
            };

            variants.push(DocumentSymbol {
                name: name.clone().to_string(),
                detail: None,
                kind: SymbolKind::ENUM_MEMBER,
                range: variant_range,
                selection_range: variant_range,
                children: None,
                tags: Some(vec![]),
                deprecated: Some(false),
            });
        }
    }
    variants
}
