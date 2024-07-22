// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module is responsible for building symbolication information on top of compiler's parsed
//! and typed ASTs, in particular identifier definitions to be used for implementing go-to-def,
//! go-to-references, and on-hover language server commands.
//!
//! There are different structs that are used at different phases of the process, the
//! ParsingSymbolicator and Typing Symbolicator structs are used when building symbolication
//! information and the Symbols struct is summarizes the symbolication results and is used by the
//! language server find definitions and references.
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
//!
//! Symbolication algorithm over typing AST first analyzes all top-level definitions from all
//! modules. ParsingSymbolicator then processes import statements (no longer available at the level
//! of typed AST) and TypingSymbolicator processes function bodies, as well as constant and struct
//! definitions. For local definitions, TypingSymbolicator builds a scope stack, entering
//! encountered definitions and matching uses to a definition in the innermost scope.

#![allow(clippy::non_canonical_partial_ord_impl)]

use crate::{
    analysis::typing_analysis,
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
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    thread,
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
    expansion::ast::{self as E, AbilitySet, ModuleIdent, ModuleIdent_, Value, Value_, Visibility},
    linters::LintLevel,
    naming::ast::{DatatypeTypeParameter, StructFields, Type, TypeName_, Type_, VariantFields},
    parser::ast::{self as P, NameAccessChain, NameAccessChain_},
    shared::{
        files::{FileId, MappedFiles},
        unique_map::UniqueMap,
        Identifier, Name, NamedAddressMap, NamedAddressMaps,
    },
    typing::{
        ast::{
            self as T, Exp, ExpListItem, ModuleDefinition, SequenceItem, SequenceItem_,
            UnannotatedExp_,
        },
        visitor::TypingVisitorContext,
    },
    unit_test::filter_test_members::UNIT_TEST_POISON_FUN_NAME,
    PASS_CFGIR, PASS_PARSER, PASS_TYPING,
};
use move_ir_types::location::*;
use move_package::{
    compilation::{build_plan::BuildPlan, compiled_package::ModuleFormat},
    resolution::resolution_graph::ResolvedGraph,
    source_package::parsed_manifest::FileName,
};
use move_symbol_pool::Symbol;

const MANIFEST_FILE_NAME: &str = "Move.toml";

#[derive(Clone)]
pub struct PrecompiledPkgDeps {
    /// Hash of the manifest file for a given package
    manifest_hash: Option<FileHash>,
    /// Hash of dependency source files
    deps_hash: String,
    /// Precompiled deps
    deps: Arc<FullyCompiledProgram>,
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
    name: Symbol,
    empty: bool,
    positional: bool,
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

impl CursorContext {
    fn new(loc: Loc) -> Self {
        CursorContext {
            module: None,
            defn_name: None,
            position: CursorPosition::Unknown,
            loc,
        }
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
/// Data used during symbolication over parsed AST
pub struct ParsingSymbolicator<'a> {
    /// Outermost definitions in a module (structs, consts, functions), keyd on a ModuleIdent
    /// string so that we can access it regardless of the ModuleIdent representation
    /// (e.g., in the parsing AST or in the typing AST)
    mod_outer_defs: &'a mut BTreeMap<String, ModuleDefs>,
    /// Mapped file information for translating locations into positions
    files: &'a MappedFiles,
    /// Associates uses for a given definition to allow displaying all references
    references: &'a mut References,
    /// Additional information about definitions
    def_info: &'a mut DefMap,
    /// A UseDefMap for a given module (needs to be appropriately set before the module
    /// processing starts)
    use_defs: UseDefMap,
    /// Current module identifier string (needs to be appropriately set before the module
    /// processing starts)
    current_mod_ident_str: Option<String>,
    /// Module name lengths in access paths for a given module (needs to be appropriately
    /// set before the module processing starts)
    alias_lengths: BTreeMap<Position, usize>,
    /// A per-package mapping from package names to their addresses (needs to be appropriately set
    /// before the package processint starts)
    pkg_addresses: &'a NamedAddressMap,
    /// Cursor contextual information, computed as part of the traversal.
    cursor: Option<&'a mut CursorContext>,
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
    /// Typed Program
    pub typed_ast: Option<T::Program>,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
enum RunnerState {
    Run(PathBuf),
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
                let type_args_str = type_args_to_ide_string(type_args, /* verbose */ true);
                let ret_type_str = ret_type_to_ide_str(ret_type, /* verbose */ true);
                write!(
                    f,
                    "{}{}fun {}::{}{}({}){}",
                    visibility_to_ide_string(visibility),
                    fun_type_to_ide_string(fun_type),
                    mod_ident_to_ide_string(mod_ident),
                    name,
                    type_args_str,
                    typed_id_list_to_ide_string(
                        arg_names, arg_types, /* separate_lines */ false,
                        /* verbose */ true
                    ),
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
                        "{}struct {}::{}{}{} {{}}",
                        visibility_to_ide_string(visibility),
                        mod_ident_to_ide_string(mod_ident),
                        name,
                        type_args_str,
                        abilities_str,
                    )
                } else {
                    write!(
                        f,
                        "{}struct {}::{}{}{} {{\n{}\n}}",
                        visibility_to_ide_string(visibility),
                        mod_ident_to_ide_string(mod_ident),
                        name,
                        type_args_str,
                        abilities_str,
                        typed_id_list_to_ide_string(
                            field_names,
                            field_types,
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
                        "{}enum {}::{}{}{} {{}}",
                        visibility_to_ide_string(visibility),
                        mod_ident_to_ide_string(mod_ident),
                        name,
                        type_args_str,
                        abilities_str,
                    )
                } else {
                    write!(
                        f,
                        "{}enum {}::{}{}{} {{\n{}\n}}",
                        visibility_to_ide_string(visibility),
                        mod_ident_to_ide_string(mod_ident),
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
                        "{}::{}::{}",
                        mod_ident_to_ide_string(mod_ident),
                        enum_name,
                        name
                    )
                } else if *positional {
                    write!(
                        f,
                        "{}::{}::{}({})",
                        mod_ident_to_ide_string(mod_ident),
                        enum_name,
                        name,
                        type_list_to_ide_string(field_types, /* verbose */ true)
                    )
                } else {
                    write!(
                        f,
                        "{}::{}::{}{{{}}}",
                        mod_ident_to_ide_string(mod_ident),
                        enum_name,
                        name,
                        typed_id_list_to_ide_string(
                            field_names,
                            field_types,
                            /* separate_lines */ false,
                            /* verbose */ true,
                        ),
                    )
                }
            }
            Self::Field(mod_ident, struct_name, name, t, _) => {
                write!(
                    f,
                    "{}::{}\n{}: {}",
                    mod_ident_to_ide_string(mod_ident),
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

pub fn type_args_to_ide_string(type_args: &[Type], verbose: bool) -> String {
    let mut type_args_str = "".to_string();
    if !type_args.is_empty() {
        type_args_str.push('<');
        type_args_str.push_str(&type_list_to_ide_string(type_args, verbose));
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
    separate_lines: bool,
    verbose: bool,
) -> String {
    names
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
        .join(if separate_lines { ",\n" } else { ", " })
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
                format!("({})", type_list_to_ide_string(ss, verbose))
            }
            TypeName_::Builtin(name) => {
                if ss.is_empty() {
                    format!("{}", name)
                } else {
                    format!("{}<{}>", name, type_list_to_ide_string(ss, verbose))
                }
            }
            TypeName_::ModuleType(sp!(_, module_ident), struct_name) => {
                let type_args = if ss.is_empty() {
                    "".to_string()
                } else {
                    format!("<{}>", type_list_to_ide_string(ss, verbose))
                };
                if verbose {
                    format!("{}::{}{}", module_ident, struct_name, type_args,)
                } else {
                    struct_name.to_string()
                }
            }
        },
        Type_::Fun(args, ret) => {
            format!(
                "|{}| -> {}",
                type_list_to_ide_string(args, verbose),
                type_to_ide_string(ret, verbose)
            )
        }
        Type_::Anything => "_".to_string(),
        Type_::Var(_) => "invalid type (var)".to_string(),
        Type_::UnresolvedError => "unknown type (unresolved)".to_string(),
    }
}

pub fn type_list_to_ide_string(types: &[Type], verbose: bool) -> String {
    types
        .iter()
        .map(|t| type_to_ide_string(t, verbose))
        .collect::<Vec<_>>()
        .join(", ")
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

pub fn mod_ident_to_ide_string(mod_ident: &E::ModuleIdent_) -> String {
    use E::Address as A;
    match mod_ident.address {
        A::Numerical {
            name: None, value, ..
        } => format!("{value}::{}", mod_ident.module).to_string(),
        A::Numerical { name: Some(n), .. } | A::NamedUnassigned(n) => {
            format!("{n}::{}", mod_ident.module).to_string()
        }
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
        pkg_deps: Arc<Mutex<BTreeMap<PathBuf, PrecompiledPkgDeps>>>,
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
                    let starting_path_opt = {
                        // hold the lock only as long as it takes to get the data, rather than through
                        // the whole symbolication process (hence a separate scope here)
                        let mut symbolicate = mtx.lock().unwrap();
                        match symbolicate.clone() {
                            RunnerState::Quit => break,
                            RunnerState::Run(root_dir) => {
                                *symbolicate = RunnerState::Wait;
                                Some(root_dir)
                            }
                            RunnerState::Wait => {
                                // wait for next request
                                symbolicate = cvar.wait(symbolicate).unwrap();
                                match symbolicate.clone() {
                                    RunnerState::Quit => break,
                                    RunnerState::Run(root_dir) => {
                                        *symbolicate = RunnerState::Wait;
                                        Some(root_dir)
                                    }
                                    RunnerState::Wait => None,
                                }
                            }
                        }
                    };
                    if let Some(starting_path) = starting_path_opt {
                        let root_dir = Self::root_dir(&starting_path);
                        if root_dir.is_none() && !missing_manifests.contains(&starting_path) {
                            eprintln!("reporting missing manifest");

                            // report missing manifest file only once to avoid cluttering IDE's UI in
                            // cases when developer indeed intended to open a standalone file that was
                            // not meant to compile
                            missing_manifests.insert(starting_path);
                            if let Err(err) = sender.send(Err(anyhow!(
                                "Unable to find package manifest. Make sure that
                            the source files are located in a sub-directory of a package containing
                            a Move.toml file. "
                            ))) {
                                eprintln!("could not pass missing manifest error: {:?}", err);
                            }
                            continue;
                        }
                        eprintln!("symbolication started");
                        let pkg_path = root_dir.unwrap();
                        match get_symbols(
                            pkg_deps.clone(),
                            ide_files_root.clone(),
                            pkg_path.as_path(),
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
                                    old_symbols_map.insert(pkg_path, new_symbols);
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
            })
            .unwrap();

        runner
    }

    pub fn run(&self, starting_path: PathBuf) {
        eprintln!("scheduling run for {:?}", starting_path);
        let (mtx, cvar) = &*self.mtx_cvar;
        let mut symbolicate = mtx.lock().unwrap();
        *symbolicate = RunnerState::Run(starting_path);
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
    fn rename_use(
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

    pub fn extend_inner(&mut self, use_defs: BTreeMap<u32, BTreeSet<UseDef>>) {
        for (k, v) in use_defs {
            self.0.entry(k).or_default().extend(v);
        }
    }

    pub fn extend(&mut self, use_defs: Self) {
        for (k, v) in use_defs.0 {
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
    pkg_dependencies: Arc<Mutex<BTreeMap<PathBuf, PrecompiledPkgDeps>>>,
) -> bool {
    let pkg_deps = pkg_dependencies.lock().unwrap();
    pkg_deps.contains_key(pkg_path)
}

/// Main driver to get symbols for the whole package. Returned symbols is an option as only the
/// correctly computed symbols should be a replacement for the old set - if symbols are not
/// actually (re)computed and the diagnostics are returned, the old symbolic information should
/// be retained even if it's getting out-of-date.
pub fn get_symbols(
    pkg_dependencies: Arc<Mutex<BTreeMap<PathBuf, PrecompiledPkgDeps>>>,
    ide_files_root: VfsPath,
    pkg_path: &Path,
    lint: LintLevel,
    cursor_info: Option<(&PathBuf, Position)>,
) -> Result<(Option<Symbols>, BTreeMap<PathBuf, Vec<Diagnostic>>)> {
    let build_config = move_package::BuildConfig {
        test_mode: true,
        install_dir: Some(tempdir().unwrap().path().to_path_buf()),
        default_flavor: Some(Flavor::Sui),
        lint_flag: lint.into(),
        skip_fetch_latest_git_deps: has_precompiled_deps(pkg_path, pkg_dependencies.clone()),
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

    let mut mapped_files: MappedFiles = MappedFiles::empty();

    // Hash dependencies so we can check if something has changed.
    let source_files = file_sources(&resolution_graph, overlay_fs_root.clone());
    let mut hasher = Sha256::new();
    source_files
        .iter()
        .filter(|(_, (_, _, is_dep))| *is_dep)
        .for_each(|(fhash, _)| hasher.update(fhash.0));
    let deps_hash = format!("{:X}", hasher.finalize());

    let compiler_flags = resolution_graph.build_options.compiler_flags().clone();
    let build_plan =
        BuildPlan::create(resolution_graph)?.set_compiler_vfs_root(overlay_fs_root.clone());
    let mut parsed_ast = None;
    let mut typed_ast = None;
    let mut compiler_info = None;
    let mut diagnostics = None;

    let mut dependencies = build_plan.compute_dependencies();
    let compiled_libs = if let Ok(deps_package_paths) = dependencies.make_deps_for_compiler() {
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

        let mut pkg_deps = pkg_dependencies.lock().unwrap();
        let compiled_deps = match pkg_deps.get(pkg_path) {
            Some(d)
                if manifest_hash.is_some()
                    && manifest_hash == d.manifest_hash
                    && deps_hash == d.deps_hash =>
            {
                eprintln!("found pre-compiled libs for {:?}", pkg_path);
                mapped_files.extend_with_duplicates(d.deps.files.clone());
                Some(d.deps.clone())
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
                mapped_files.extend_with_duplicates(libs.files.clone());
                let deps = Arc::new(libs);
                pkg_deps.insert(
                    pkg_path.to_path_buf(),
                    PrecompiledPkgDeps {
                        manifest_hash,
                        deps_hash,
                        deps: deps.clone(),
                    },
                );
                deps
            }),
        };
        if compiled_deps.is_some() {
            // if successful, remove only source deps but keep bytecode deps as they
            // were not used to construct pre-compiled lib in the first place
            dependencies.remove_deps(src_names);
        }
        compiled_deps
    } else {
        None
    };

    let mut edition = None;
    build_plan.compile_with_driver_and_deps(dependencies, &mut std::io::sink(), |compiler| {
        let compiler = compiler.set_ide_mode();
        // extract expansion AST
        let (files, compilation_result) = compiler
            .set_pre_compiled_lib_opt(compiled_libs.clone())
            .run::<PASS_PARSER>()?;
        let (_, compiler) = match compilation_result {
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
        mapped_files.extend_with_duplicates(compiler.compilation_env_ref().mapped_files().clone());

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
        let (mut compiler, typed_program) = compiler.into_ast();
        typed_ast = Some(typed_program.clone());
        compiler_info = Some(CompilerInfo::from(
            compiler.compilation_env().ide_information.clone(),
        ));
        edition = Some(compiler.compilation_env().edition(Some(root_pkg_name)));

        // compile to CFGIR for accurate diags
        eprintln!("compiling to CFGIR");
        let compilation_result = compiler.at_typing(typed_program).run::<PASS_CFGIR>();
        let mut compiler = match compilation_result {
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
    let parsed_program = parsed_ast.unwrap();
    let mut typed_program = typed_ast.clone().unwrap();

    let mut mod_outer_defs = BTreeMap::new();
    let mut mod_use_defs = BTreeMap::new();
    let mut references = BTreeMap::new();
    let mut def_info = BTreeMap::new();

    let mut file_id_to_lines = HashMap::new();
    for file_id in mapped_files.file_mapping().values() {
        let Ok(file) = mapped_files.files().get(*file_id) else {
            eprintln!("file id without source code");
            continue;
        };
        let source = file.source();
        let lines: Vec<String> = source.lines().map(String::from).collect();
        file_id_to_lines.insert(*file_id, lines);
    }

    let mut cursor_context = compute_cursor_context(&mapped_files, cursor_info);

    pre_process_typed_modules(
        &typed_program.modules,
        &mapped_files,
        &file_id_to_lines,
        &mut mod_outer_defs,
        &mut mod_use_defs,
        &mut references,
        &mut def_info,
        &edition,
        cursor_context.as_mut(),
    );

    if let Some(libs) = compiled_libs.clone() {
        pre_process_typed_modules(
            &libs.typing.modules,
            &mapped_files,
            &file_id_to_lines,
            &mut mod_outer_defs,
            &mut mod_use_defs,
            &mut references,
            &mut def_info,
            &edition,
            None, // Cursor can never be in a compiled library(?)
        );
    }

    eprintln!("get_symbols loaded");

    let mut file_use_defs = BTreeMap::new();
    let mut mod_to_alias_lengths = BTreeMap::new();

    let mut parsing_symbolicator = ParsingSymbolicator {
        mod_outer_defs: &mut mod_outer_defs,
        files: &mapped_files,
        references: &mut references,
        def_info: &mut def_info,
        use_defs: UseDefMap::new(),
        current_mod_ident_str: None,
        alias_lengths: BTreeMap::new(),
        pkg_addresses: &NamedAddressMap::new(),
        cursor: cursor_context.as_mut(),
    };

    parsing_symbolicator.prog_symbols(
        &parsed_program,
        &mut mod_use_defs,
        &mut mod_to_alias_lengths,
    );
    if let Some(libs) = compiled_libs.clone() {
        parsing_symbolicator.cursor = None;
        parsing_symbolicator.prog_symbols(
            &libs.parser,
            &mut mod_use_defs,
            &mut mod_to_alias_lengths,
        );
    }

    let mut compiler_info = compiler_info.unwrap();
    let mut typing_symbolicator = typing_analysis::TypingAnalysisContext {
        mod_outer_defs: &mut mod_outer_defs,
        files: &mapped_files,
        references: &mut references,
        def_info: &mut def_info,
        use_defs: UseDefMap::new(),
        current_mod_ident_str: None,
        alias_lengths: &BTreeMap::new(),
        traverse_only: false,
        compiler_info: &mut compiler_info,
        type_params: BTreeMap::new(),
        expression_scope: OrdMap::new(),
    };

    process_typed_modules(
        &mut typed_program.modules,
        &source_files,
        &mod_to_alias_lengths,
        &mut typing_symbolicator,
        &mut file_use_defs,
        &mut mod_use_defs,
    );

    if let Some(libs) = compiled_libs {
        process_typed_modules(
            &mut libs.typing.modules.clone(),
            &source_files,
            &mod_to_alias_lengths,
            &mut typing_symbolicator,
            &mut file_use_defs,
            &mut mod_use_defs,
        );
    }

    let mut file_mods: FileModules = BTreeMap::new();
    for d in mod_outer_defs.into_values() {
        let path = mapped_files.file_path(&d.fhash.clone());
        file_mods.entry(path.to_path_buf()).or_default().insert(d);
    }

    let symbols = Symbols {
        references,
        file_use_defs,
        file_mods,
        def_info,
        files: mapped_files,
        compiler_info,
        cursor_context,
        typed_ast,
    };

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

fn pre_process_typed_modules(
    typed_modules: &UniqueMap<ModuleIdent, ModuleDefinition>,
    files: &MappedFiles,
    file_id_to_lines: &HashMap<usize, Vec<String>>,
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
            module_def,
            files,
            file_id_to_lines,
            references,
            def_info,
            edition,
        );
        mod_outer_defs.insert(mod_ident_str.clone(), defs);
        mod_use_defs.insert(mod_ident_str, symbols);
    }
}

fn process_typed_modules<'a>(
    typed_modules: &mut UniqueMap<ModuleIdent, ModuleDefinition>,
    source_files: &BTreeMap<FileHash, (Symbol, String, bool)>,
    mod_to_alias_lengths: &'a BTreeMap<String, BTreeMap<Position, usize>>,
    typing_symbolicator: &mut typing_analysis::TypingAnalysisContext<'a>,
    file_use_defs: &mut FileUseDefs,
    mod_use_defs: &mut BTreeMap<String, UseDefMap>,
) {
    for (module_ident, module_def) in typed_modules.key_cloned_iter_mut() {
        let mod_ident_str = expansion_mod_ident_to_map_key(&module_ident.value);
        typing_symbolicator.use_defs = mod_use_defs.remove(&mod_ident_str).unwrap();
        typing_symbolicator.alias_lengths = mod_to_alias_lengths.get(&mod_ident_str).unwrap();
        typing_symbolicator.visit_module(module_ident, module_def);

        let fpath = match source_files.get(&module_ident.loc.file_hash()) {
            Some((p, _, _)) => p,
            None => continue,
        };

        let fpath_buffer =
            dunce::canonicalize(fpath.as_str()).unwrap_or_else(|_| PathBuf::from(fpath.as_str()));

        let use_defs = std::mem::replace(&mut typing_symbolicator.use_defs, UseDefMap::new());
        file_use_defs
            .entry(fpath_buffer)
            .or_default()
            .extend_inner(use_defs.elements());
    }
}

fn file_sources(
    resolved_graph: &ResolvedGraph,
    overlay_fs: VfsPath,
) -> BTreeMap<FileHash, (FileName, String, bool)> {
    resolved_graph
        .package_table
        .iter()
        .flat_map(|(_, rpkg)| {
            rpkg.get_sources(&resolved_graph.build_options)
                .unwrap()
                .iter()
                .map(|f| {
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
                    // write to top layer of the overlay file system so that the content
                    // is immutable for the duration of compilation and symbolication
                    let _ = vfs_file_path.parent().create_dir_all();
                    let mut vfs_file = vfs_file_path.create_file().unwrap();
                    let _ = vfs_file.write_all(contents.as_bytes());
                    (fhash, (Symbol::from(fname), contents, is_dep))
                })
                .collect::<BTreeMap<_, _>>()
        })
        .collect()
}

/// Produces module ident string of the form pkg::module to be used as a map key.
/// It's important that these are consistent between parsing AST and typed AST,
fn parsing_mod_ident_to_map_key(
    pkg_addresses: &NamedAddressMap,
    mod_ident: &P::ModuleIdent_,
) -> String {
    format!(
        "{}::{}",
        parsed_address(mod_ident.address, pkg_addresses),
        mod_ident.module
    )
    .to_string()
}

/// Produces module ident string of the form pkg::module to be used as a map key.
/// It's important that these are consistent between parsing AST and typed AST.
fn parsing_mod_def_to_map_key(
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

/// Produces module ident string of the form pkg::module to be used as a map key.
/// It's important that these are consistent between parsing AST and typed AST.
fn parsing_leading_and_mod_names_to_map_key(
    pkg_addresses: &NamedAddressMap,
    ln: P::LeadingNameAccess,
    name: P::ModuleName,
) -> String {
    format!("{}::{}", parsed_address(ln, pkg_addresses), name).to_string()
}

/// Converts parsing AST's `LeadingNameAccess` to expansion AST's `Address` (similarly to
/// expansion::translate::top_level_address but disregarding the name portion of `Address` as we
/// only care about actual address here if it's available). We need this to be able to reliably
/// compare parsing AST's module identifier with expansion/typing AST's module identifier, even in
/// presence of module renaming (i.e., we cannot rely on module names if addresses are available).
fn parsed_address(ln: P::LeadingNameAccess, pkg_addresses: &NamedAddressMap) -> E::Address {
    let sp!(loc, ln_) = ln;
    match ln_ {
        P::LeadingNameAccess_::AnonymousAddress(bytes) => E::Address::anonymous(loc, bytes),
        P::LeadingNameAccess_::GlobalAddress(name) => E::Address::NamedUnassigned(name),
        P::LeadingNameAccess_::Name(name) => match pkg_addresses.get(&name.value).copied() {
            Some(addr) => E::Address::anonymous(loc, addr),
            None => E::Address::NamedUnassigned(name),
        },
    }
}

/// Produces module ident string of the form pkg::module to be used as a map key
/// It's important that these are consistent between parsing AST and typed AST.
pub fn expansion_mod_ident_to_map_key(mod_ident: &E::ModuleIdent_) -> String {
    use E::Address as A;
    match mod_ident.address {
        A::Numerical { value, .. } => format!("{value}::{}", mod_ident.module).to_string(),
        A::NamedUnassigned(n) => format!("{n}::{}", mod_ident.module).to_string(),
    }
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
        typed_ast: None,
    }
}

/// Some functions defined in a module need to be ignored.
fn ignored_function(name: Symbol) -> bool {
    // In test mode (that's how IDE compiles Move source files),
    // the compiler inserts an dummy function preventing preventing
    // publishing of modules compiled in test mode. We need to
    // ignore its definition to avoid spurious on-hover display
    // of this function's info whe hovering close to `module` keyword.
    name == UNIT_TEST_POISON_FUN_NAME
}

/// Main AST traversal functions

fn field_defs_and_types(
    datatype_name: Symbol,
    datatype_loc: Loc,
    fields: &E::Fields<Type>,
    mod_ident: &ModuleIdent,
    files: &MappedFiles,
    file_id_to_lines: &HashMap<usize, Vec<String>>,
    def_info: &mut DefMap,
) -> (Vec<FieldDef>, Vec<Type>) {
    let mut field_defs = vec![];
    let mut field_types = vec![];
    for (floc, fname, (_, t)) in fields {
        field_defs.push(FieldDef {
            name: *fname,
            loc: floc,
        });
        let doc_string = extract_doc_string(files, file_id_to_lines, &floc, Some(datatype_loc));
        def_info.insert(
            floc,
            DefInfo::Field(
                mod_ident.value,
                datatype_name,
                *fname,
                t.clone(),
                doc_string,
            ),
        );
        field_types.push(t.clone());
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

/// Get symbols for outer definitions in the module (functions, structs, and consts)
fn get_mod_outer_defs(
    loc: &Loc,
    mod_ident: &ModuleIdent,
    mod_def: &ModuleDefinition,
    files: &MappedFiles,
    file_id_to_lines: &HashMap<usize, Vec<String>>,
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
            (field_defs, field_types) = field_defs_and_types(
                *name,
                name_loc,
                fields,
                mod_ident,
                files,
                file_id_to_lines,
                def_info,
            );
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
        let doc_string = extract_doc_string(files, file_id_to_lines, &name_loc, None);
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
                    let (defs, types) = field_defs_and_types(
                        *name,
                        name_loc,
                        fields,
                        mod_ident,
                        files,
                        file_id_to_lines,
                        def_info,
                    );
                    (defs, types, *pos_fields)
                }
                VariantFields::Empty => (vec![], vec![], false),
            };
            let field_names = field_defs.iter().map(|f| sp(f.loc, f.name)).collect();
            def_info_variants.push(VariantInfo {
                name: *vname,
                empty: field_defs.is_empty(),
                positional,
            });
            variants_info.insert(*vname, (vname_loc, field_defs, positional));

            let vdoc_string =
                extract_doc_string(files, file_id_to_lines, &vname_loc, Some(name_loc));
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
        let enum_doc_string = extract_doc_string(files, file_id_to_lines, &name_loc, None);
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
        let doc_string = extract_doc_string(files, file_id_to_lines, &name_loc, None);
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
        let doc_string = extract_doc_string(files, file_id_to_lines, &name_loc, None);
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
    let doc_comment = extract_doc_string(files, file_id_to_lines, loc, None);
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
            DefInfo::Module(mod_ident_to_ide_string(&ident), doc_comment),
        );
    }

    (mod_defs, use_def_map)
}

macro_rules! update_cursor {
    ($cursor:expr, $subject:expr, $kind:ident) => {
        if let Some(cursor) = &mut $cursor {
            if $subject.loc.contains(&cursor.loc) {
                cursor.position = CursorPosition::$kind($subject.clone());
            }
        };
    };
    (IDENT, $cursor:expr, $subject:expr, $kind:ident) => {
        if let Some(cursor) = &mut $cursor {
            if $subject.loc().contains(&cursor.loc) {
                cursor.position = CursorPosition::$kind($subject.clone());
            }
        };
    };
}

impl<'a> ParsingSymbolicator<'a> {
    /// Get symbols for the whole program
    fn prog_symbols(
        &mut self,
        prog: &'a P::Program,
        mod_use_defs: &mut BTreeMap<String, UseDefMap>,
        mod_to_alias_lengths: &mut BTreeMap<String, BTreeMap<Position, usize>>,
    ) {
        prog.source_definitions.iter().for_each(|pkg_def| {
            self.pkg_symbols(
                &prog.named_address_maps,
                pkg_def,
                mod_use_defs,
                mod_to_alias_lengths,
            )
        });
        prog.lib_definitions.iter().for_each(|pkg_def| {
            self.pkg_symbols(
                &prog.named_address_maps,
                pkg_def,
                mod_use_defs,
                mod_to_alias_lengths,
            )
        });
    }

    /// Get symbols for the whole package
    fn pkg_symbols(
        &mut self,
        pkg_address_maps: &'a NamedAddressMaps,
        pkg_def: &P::PackageDefinition,
        mod_use_defs: &mut BTreeMap<String, UseDefMap>,
        mod_to_alias_lengths: &mut BTreeMap<String, BTreeMap<Position, usize>>,
    ) {
        if let P::Definition::Module(mod_def) = &pkg_def.def {
            let pkg_addresses = pkg_address_maps.get(pkg_def.named_address_map);
            let old_addresses = std::mem::replace(&mut self.pkg_addresses, pkg_addresses);
            self.mod_symbols(mod_def, mod_use_defs, mod_to_alias_lengths);
            self.current_mod_ident_str = None;
            let _ = std::mem::replace(&mut self.pkg_addresses, old_addresses);
        }
    }

    /// Get symbols for the whole module
    fn mod_symbols(
        &mut self,
        mod_def: &P::ModuleDefinition,
        mod_use_defs: &mut BTreeMap<String, UseDefMap>,
        mod_to_alias_lengths: &mut BTreeMap<String, BTreeMap<Position, usize>>,
    ) {
        // parsing symbolicator is currently only responsible for processing use declarations
        let Some(mod_ident_str) = parsing_mod_def_to_map_key(self.pkg_addresses, mod_def) else {
            return;
        };
        assert!(self.current_mod_ident_str.is_none());
        self.current_mod_ident_str = Some(mod_ident_str.clone());

        let use_defs = mod_use_defs.remove(&mod_ident_str).unwrap();
        let old_defs = std::mem::replace(&mut self.use_defs, use_defs);
        let alias_lengths: BTreeMap<Position, usize> = BTreeMap::new();
        let old_alias_lengths = std::mem::replace(&mut self.alias_lengths, alias_lengths);

        for m in &mod_def.members {
            use P::ModuleMember as MM;
            match m {
                MM::Function(fun) => {
                    if ignored_function(fun.name.value()) {
                        continue;
                    }

                    // Unit returns span the entire function signature, so we process them first
                    // for cursor ordering.
                    self.type_symbols(&fun.signature.return_type);

                    // If the cursor is in this item, mark that down.
                    // This may be overridden by the recursion below.
                    if let Some(cursor) = &mut self.cursor {
                        if fun.name.loc().contains(&cursor.loc) {
                            cursor.position = CursorPosition::DefName;
                            debug_assert!(cursor.defn_name.is_none());
                            cursor.defn_name = Some(CursorDefinition::Function(fun.name));
                        } else if fun.loc.contains(&cursor.loc) {
                            cursor.defn_name = Some(CursorDefinition::Function(fun.name));
                        }
                    };

                    for (_, x, t) in fun.signature.parameters.iter() {
                        update_cursor!(IDENT, self.cursor, x, Parameter);
                        self.type_symbols(t)
                    }

                    if fun.macro_.is_some() {
                        // we currently do not process macro function bodies
                        // in the parsing symbolicator (and do very limited
                        // processing in typing symbolicator)
                        continue;
                    }
                    if let P::FunctionBody_::Defined(seq) = &fun.body.value {
                        self.seq_symbols(seq);
                    };
                }
                MM::Struct(sdef) => {
                    // If the cursor is in this item, mark that down.
                    // This may be overridden by the recursion below.
                    if let Some(cursor) = &mut self.cursor {
                        if sdef.name.loc().contains(&cursor.loc) {
                            cursor.position = CursorPosition::DefName;
                            debug_assert!(cursor.defn_name.is_none());
                            cursor.defn_name = Some(CursorDefinition::Struct(sdef.name));
                        } else if sdef.loc.contains(&cursor.loc) {
                            cursor.defn_name = Some(CursorDefinition::Struct(sdef.name));
                        }
                    };
                    match &sdef.fields {
                        P::StructFields::Named(v) => v.iter().for_each(|(x, t)| {
                            self.field_defn(x);
                            self.type_symbols(t)
                        }),
                        P::StructFields::Positional(v) => {
                            v.iter().for_each(|t| self.type_symbols(t))
                        }
                        P::StructFields::Native(_) => (),
                    }
                }
                MM::Enum(edef) => {
                    // If the cursor is in this item, mark that down.
                    // This may be overridden by the recursion below.
                    if let Some(cursor) = &mut self.cursor {
                        if edef.name.loc().contains(&cursor.loc) {
                            cursor.position = CursorPosition::DefName;
                            debug_assert!(cursor.defn_name.is_none());
                            cursor.defn_name = Some(CursorDefinition::Enum(edef.name));
                        } else if edef.loc.contains(&cursor.loc) {
                            cursor.defn_name = Some(CursorDefinition::Enum(edef.name));
                        }
                    };

                    let P::EnumDefinition { variants, .. } = edef;
                    for variant in variants {
                        let P::VariantDefinition { fields, .. } = variant;
                        match fields {
                            P::VariantFields::Named(v) => v.iter().for_each(|(x, t)| {
                                self.field_defn(x);
                                self.type_symbols(t)
                            }),
                            P::VariantFields::Positional(v) => {
                                v.iter().for_each(|t| self.type_symbols(t))
                            }
                            P::VariantFields::Empty => (),
                        }
                    }
                }
                MM::Use(use_decl) => self.use_decl_symbols(use_decl),
                MM::Friend(fdecl) => self.chain_symbols(&fdecl.friend),
                MM::Constant(c) => {
                    // If the cursor is in this item, mark that down.
                    // This may be overridden by the recursion below.
                    if let Some(cursor) = &mut self.cursor {
                        if c.name.loc().contains(&cursor.loc) {
                            cursor.position = CursorPosition::DefName;
                            debug_assert!(cursor.defn_name.is_none());
                            cursor.defn_name = Some(CursorDefinition::Constant(c.name));
                        } else if c.loc.contains(&cursor.loc) {
                            cursor.defn_name = Some(CursorDefinition::Constant(c.name));
                        }
                    };
                    self.type_symbols(&c.signature);
                    self.exp_symbols(&c.value);
                }
                MM::Spec(_) => (),
            }
        }
        self.current_mod_ident_str = None;
        let processed_defs = std::mem::replace(&mut self.use_defs, old_defs);
        mod_use_defs.insert(mod_ident_str.clone(), processed_defs);
        let processed_alias_lengths = std::mem::replace(&mut self.alias_lengths, old_alias_lengths);
        mod_to_alias_lengths.insert(mod_ident_str, processed_alias_lengths);
    }

    /// Get symbols for a sequence item
    fn seq_item_symbols(&mut self, seq_item: &P::SequenceItem) {
        use P::SequenceItem_ as I;

        // If the cursor is in this item, mark that down.
        // This may be overridden by the recursion below.
        update_cursor!(self.cursor, seq_item, SeqItem);

        match &seq_item.value {
            I::Seq(e) => self.exp_symbols(e),
            I::Declare(v, to) => {
                v.value
                    .iter()
                    .for_each(|bind| self.bind_symbols(bind, to.is_some()));
                if let Some(t) = to {
                    self.type_symbols(t);
                }
            }
            I::Bind(v, to, e) => {
                v.value
                    .iter()
                    .for_each(|bind| self.bind_symbols(bind, to.is_some()));
                if let Some(t) = to {
                    self.type_symbols(t);
                }
                self.exp_symbols(e);
            }
        }
    }

    fn path_entry_symbols(&mut self, path: &P::PathEntry) {
        let P::PathEntry {
            name: _,
            tyargs,
            is_macro: _,
        } = path;
        if let Some(sp!(_, tyargs)) = tyargs {
            tyargs.iter().for_each(|t| self.type_symbols(t));
        }
    }

    fn root_path_entry_symbols(&mut self, path: &P::RootPathEntry) {
        let P::RootPathEntry {
            name: _,
            tyargs,
            is_macro: _,
        } = path;
        if let Some(sp!(_, tyargs)) = tyargs {
            tyargs.iter().for_each(|t| self.type_symbols(t));
        }
    }

    /// Get symbols for an expression
    fn exp_symbols(&mut self, exp: &P::Exp) {
        use P::Exp_ as E;
        fn last_chain_symbol_loc(sp!(_, chain): &NameAccessChain) -> Loc {
            use NameAccessChain_ as NA;
            match chain {
                NA::Single(entry) => entry.name.loc,
                NA::Path(path) => {
                    if path.entries.is_empty() {
                        path.root.name.loc
                    } else {
                        path.entries.last().unwrap().name.loc
                    }
                }
            }
        }

        // If the cursor is in this item, mark that down.
        // This may be overridden by the recursion below.
        update_cursor!(self.cursor, exp, Exp);

        match &exp.value {
            E::Move(_, e) => self.exp_symbols(e),
            E::Copy(_, e) => self.exp_symbols(e),
            E::Name(chain) => self.chain_symbols(chain),
            E::Call(chain, v) => {
                self.chain_symbols(chain);
                v.value.iter().for_each(|e| self.exp_symbols(e));
                assert!(self.current_mod_ident_str.is_some());
                if let Some(mod_defs) = self
                    .mod_outer_defs
                    .get_mut(&self.current_mod_ident_str.clone().unwrap())
                {
                    mod_defs.call_infos.insert(
                        last_chain_symbol_loc(chain),
                        CallInfo::new(/* do_call */ false, &v.value),
                    );
                };
            }
            E::Pack(chain, v) => {
                self.chain_symbols(chain);
                v.iter().for_each(|(_, e)| self.exp_symbols(e));
            }
            E::Vector(_, vo, v) => {
                if let Some(v) = vo {
                    v.iter().for_each(|t| self.type_symbols(t));
                }
                v.value.iter().for_each(|e| self.exp_symbols(e));
            }
            E::IfElse(e1, e2, oe) => {
                self.exp_symbols(e1);
                self.exp_symbols(e2);
                if let Some(e) = oe.as_ref() {
                    self.exp_symbols(e)
                }
            }
            E::Match(e, sp!(_, v)) => {
                self.exp_symbols(e);
                v.iter().for_each(|sp!(_, arm)| {
                    self.match_pattern_symbols(&arm.pattern);
                    if let Some(g) = &arm.guard {
                        self.exp_symbols(g);
                    }
                    self.exp_symbols(&arm.rhs);
                })
            }
            E::While(e1, e2) => {
                self.exp_symbols(e1);
                self.exp_symbols(e2);
            }
            E::Loop(e) => self.exp_symbols(e),
            E::Labeled(_, e) => self.exp_symbols(e),
            E::Block(seq) => self.seq_symbols(seq),
            E::Lambda(sp!(_, bindings), to, e) => {
                for (sp!(_, v), bto) in bindings {
                    if let Some(bt) = bto {
                        self.type_symbols(bt);
                    }
                    v.iter()
                        .for_each(|bind| self.bind_symbols(bind, to.is_some()));
                }
                if let Some(t) = to {
                    self.type_symbols(t);
                }
                self.exp_symbols(e);
            }
            E::ExpList(l) => l.iter().for_each(|e| self.exp_symbols(e)),
            E::Parens(e) => self.exp_symbols(e),
            E::Assign(e1, e2) => {
                self.exp_symbols(e1);
                self.exp_symbols(e2);
            }
            E::Abort(e) => self.exp_symbols(e),
            E::Return(_, oe) => {
                if let Some(e) = oe.as_ref() {
                    self.exp_symbols(e)
                }
            }
            E::Break(_, oe) => {
                if let Some(e) = oe.as_ref() {
                    self.exp_symbols(e)
                }
            }
            E::Dereference(e) => self.exp_symbols(e),
            E::UnaryExp(_, e) => self.exp_symbols(e),
            E::BinopExp(e1, _, e2) => {
                self.exp_symbols(e1);
                self.exp_symbols(e2);
            }
            E::Borrow(_, e) => self.exp_symbols(e),
            E::Dot(e, _) => self.exp_symbols(e),
            E::DotCall(e, name, _, vo, v) => {
                self.exp_symbols(e);
                if let Some(v) = vo {
                    v.iter().for_each(|t| self.type_symbols(t));
                }
                v.value.iter().for_each(|e| self.exp_symbols(e));
                assert!(self.current_mod_ident_str.is_some());
                if let Some(mod_defs) = self
                    .mod_outer_defs
                    .get_mut(&self.current_mod_ident_str.clone().unwrap())
                {
                    mod_defs
                        .call_infos
                        .insert(name.loc, CallInfo::new(/* do_call */ true, &v.value));
                };
            }
            E::Index(e, v) => {
                self.exp_symbols(e);
                v.value.iter().for_each(|e| self.exp_symbols(e));
            }
            E::Cast(e, t) => {
                self.exp_symbols(e);
                self.type_symbols(t);
            }
            E::Annotate(e, t) => {
                self.exp_symbols(e);
                self.type_symbols(t);
            }
            E::DotUnresolved(_, e) => self.exp_symbols(e),
            E::Value(_)
            | E::Quant(..)
            | E::Unit
            | E::Continue(_)
            | E::Spec(_)
            | E::UnresolvedError => (),
        }
    }

    fn match_pattern_symbols(&mut self, sp!(_, pattern): &P::MatchPattern) {
        use P::MatchPattern_ as MP;
        match pattern {
            MP::PositionalConstructor(chain, sp!(_, v)) => {
                self.chain_symbols(chain);
                v.iter().for_each(|e| {
                    if let P::Ellipsis::Binder(m) = e {
                        self.match_pattern_symbols(m);
                    }
                })
            }
            MP::FieldConstructor(chain, sp!(_, v)) => {
                self.chain_symbols(chain);
                v.iter().for_each(|e| {
                    if let P::Ellipsis::Binder((_, m)) = e {
                        self.match_pattern_symbols(m);
                    }
                })
            }
            MP::Name(_, chain) => {
                self.chain_symbols(chain);
                assert!(self.current_mod_ident_str.is_some());
                if let Some(mod_defs) = self
                    .mod_outer_defs
                    .get_mut(&self.current_mod_ident_str.clone().unwrap())
                {
                    mod_defs.untyped_defs.insert(chain.loc);
                };
            }
            MP::Or(m1, m2) => {
                self.match_pattern_symbols(m2);
                self.match_pattern_symbols(m1);
            }
            MP::At(_, m) => self.match_pattern_symbols(m),
            MP::Literal(_) => (),
        }
    }

    /// Get symbols for a sequence
    fn seq_symbols(&mut self, (use_decls, seq_items, _, oe): &P::Sequence) {
        use_decls
            .iter()
            .for_each(|use_decl| self.use_decl_symbols(use_decl));

        seq_items
            .iter()
            .for_each(|seq_item| self.seq_item_symbols(seq_item));
        if let Some(e) = oe.as_ref().as_ref() {
            self.exp_symbols(e)
        }
    }

    /// Get symbols for a use declaration
    fn use_decl_symbols(&mut self, use_decl: &P::UseDecl) {
        match &use_decl.use_ {
            P::Use::ModuleUse(mod_ident, mod_use) => {
                let mod_ident_str =
                    parsing_mod_ident_to_map_key(self.pkg_addresses, &mod_ident.value);
                self.mod_name_symbol(&mod_ident.value.module, &mod_ident_str);
                self.mod_use_symbols(mod_use, &mod_ident_str);
            }
            P::Use::NestedModuleUses(leading_name, uses) => {
                for (mod_name, mod_use) in uses {
                    let mod_ident_str = parsing_leading_and_mod_names_to_map_key(
                        self.pkg_addresses,
                        *leading_name,
                        *mod_name,
                    );

                    self.mod_name_symbol(mod_name, &mod_ident_str);
                    self.mod_use_symbols(mod_use, &mod_ident_str);
                }
            }
            P::Use::Fun {
                visibility: _,
                function,
                ty,
                method: _,
            } => {
                self.chain_symbols(function);
                self.chain_symbols(ty);
            }
        }
    }

    /// Get module name symbol
    fn mod_name_symbol(&mut self, mod_name: &P::ModuleName, mod_ident_str: &String) {
        let Some(mod_defs) = self.mod_outer_defs.get_mut(mod_ident_str) else {
            return;
        };
        let Some(mod_name_start) = loc_start_to_lsp_position_opt(self.files, &mod_name.loc())
        else {
            debug_assert!(false);
            return;
        };
        self.use_defs.insert(
            mod_name_start.line,
            UseDef::new(
                self.references,
                &BTreeMap::new(),
                mod_name.loc().file_hash(),
                mod_name_start,
                mod_defs.name_loc,
                &mod_name.value(),
                None,
            ),
        );
    }

    /// Get symbols for a module use
    fn mod_use_symbols(&mut self, mod_use: &P::ModuleUse, mod_ident_str: &String) {
        match mod_use {
            P::ModuleUse::Module(Some(alias_name)) => {
                self.mod_name_symbol(alias_name, mod_ident_str);
            }
            P::ModuleUse::Module(None) => (), // nothing more to do
            P::ModuleUse::Members(v) => {
                for (name, alias_opt) in v {
                    self.use_decl_member_symbols(mod_ident_str.clone(), name, alias_opt);
                }
            }
        }
    }

    /// Get symbols for a module member in the use declaration (can be a struct or a function)
    fn use_decl_member_symbols(
        &mut self,
        mod_ident_str: String,
        name: &Name,
        alias_opt: &Option<Name>,
    ) {
        let Some(mod_defs) = self.mod_outer_defs.get(&mod_ident_str) else {
            return;
        };
        if let Some(mut ud) = add_member_use_def(
            &name.value,
            self.files,
            mod_defs,
            &name.value,
            &name.loc,
            self.references,
            self.def_info,
            &mut self.use_defs,
            &BTreeMap::new(),
        ) {
            // it's a struct - add it for the alias as well
            if let Some(alias) = alias_opt {
                let Some(alias_start) = loc_start_to_lsp_position_opt(self.files, &alias.loc)
                else {
                    debug_assert!(false);
                    return;
                };
                ud.rename_use(
                    self.references,
                    alias.value,
                    alias_start,
                    alias.loc.file_hash(),
                );
                self.use_defs.insert(alias_start.line, ud);
            }
            return;
        }
        if let Some(mut ud) = add_member_use_def(
            &name.value,
            self.files,
            mod_defs,
            &name.value,
            &name.loc,
            self.references,
            self.def_info,
            &mut self.use_defs,
            &BTreeMap::new(),
        ) {
            // it's a function - add it for the alias as well
            if let Some(alias) = alias_opt {
                let Some(alias_start) = loc_start_to_lsp_position_opt(self.files, &alias.loc)
                else {
                    debug_assert!(false);
                    return;
                };
                ud.rename_use(
                    self.references,
                    alias.value,
                    alias_start,
                    alias.loc.file_hash(),
                );
                self.use_defs.insert(alias_start.line, ud);
            }
        }
    }

    /// Get symbols for a type
    fn type_symbols(&mut self, type_: &P::Type) {
        use P::Type_ as T;

        // If the cursor is in this item, mark that down.
        // This may be overridden by the recursion below.
        update_cursor!(self.cursor, type_, Type);

        match &type_.value {
            T::Apply(chain) => {
                self.chain_symbols(chain);
            }
            T::Ref(_, t) => self.type_symbols(t),
            T::Fun(v, t) => {
                v.iter().for_each(|t| self.type_symbols(t));
                self.type_symbols(t);
            }
            T::Multiple(v) => v.iter().for_each(|t| self.type_symbols(t)),
            T::Unit => (),
            T::UnresolvedError => (),
        }
    }

    /// Get symbols for a bind statement
    fn bind_symbols(&mut self, bind: &P::Bind, explicitly_typed: bool) {
        use P::Bind_ as B;

        // If the cursor is in this item, mark that down.
        // This may be overridden by the recursion below.
        update_cursor!(self.cursor, bind, Binding);

        match &bind.value {
            B::Unpack(chain, bindings) => {
                self.chain_symbols(chain);
                match bindings {
                    P::FieldBindings::Named(v) => {
                        for symbol in v {
                            match symbol {
                                P::Ellipsis::Binder((_, x)) => self.bind_symbols(x, false),
                                P::Ellipsis::Ellipsis(_) => (),
                            }
                        }
                    }
                    P::FieldBindings::Positional(v) => {
                        for symbol in v.iter() {
                            match symbol {
                                P::Ellipsis::Binder(x) => self.bind_symbols(x, false),
                                P::Ellipsis::Ellipsis(_) => (),
                            }
                        }
                    }
                }
            }
            B::Var(_, var) => {
                if !explicitly_typed {
                    assert!(self.current_mod_ident_str.is_some());
                    if let Some(mod_defs) = self
                        .mod_outer_defs
                        .get_mut(&self.current_mod_ident_str.clone().unwrap())
                    {
                        mod_defs.untyped_defs.insert(var.loc());
                    };
                }
            }
        }
    }

    /// Get symbols for a name access chain
    fn chain_symbols(&mut self, sp!(_, chain): &P::NameAccessChain) {
        use P::NameAccessChain_ as NA;
        // Record the length of all identifiers representing a potentially
        // aliased module, struct, enum or function name in an access chain.
        // We can conservatively record all identifiers as they are only
        // accessed by-location so those irrelevant will never be queried.
        match chain {
            NA::Single(entry) => {
                self.path_entry_symbols(entry);
                if let Some(loc) = loc_start_to_lsp_position_opt(self.files, &entry.name.loc) {
                    self.alias_lengths.insert(loc, entry.name.value.len());
                };
            }
            NA::Path(path) => {
                let P::NamePath {
                    root,
                    entries,
                    is_incomplete: _,
                } = path;
                self.root_path_entry_symbols(root);
                if let Some(root_loc) = loc_start_to_lsp_position_opt(self.files, &root.name.loc) {
                    if let P::LeadingNameAccess_::Name(n) = root.name.value {
                        self.alias_lengths.insert(root_loc, n.value.len());
                    }
                };
                entries.iter().for_each(|entry| {
                    self.path_entry_symbols(entry);
                    if let Some(loc) = loc_start_to_lsp_position_opt(self.files, &entry.name.loc) {
                        self.alias_lengths.insert(loc, entry.name.value.len());
                    };
                });
            }
        };
    }

    fn field_defn(&mut self, field: &P::Field) {
        // If the cursor is in this item, mark that down.
        update_cursor!(IDENT, self.cursor, field, FieldDefn);
    }
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

/// Extracts the docstring (/// or /** ... */) for a given definition by traversing up from the line definition
fn extract_doc_string(
    files: &MappedFiles,
    file_id_to_lines: &HashMap<FileId, Vec<String>>,
    loc: &Loc,
    outer_def_loc: Option<Loc>,
) -> Option<String> {
    let file_hash = loc.file_hash();
    let file_id = files.file_hash_to_file_id(&file_hash)?;
    let start_position = files.start_position_opt(loc)?;
    let file_lines = file_id_to_lines.get(&file_id)?;

    if let Some(outer_loc) = outer_def_loc {
        if let Some(outer_pos) = files.start_position_opt(&outer_loc) {
            if outer_pos.line_offset() == start_position.line_offset() {
                // It's a bit of a hack but due to the way we extract doc strings
                // we should not do it for a definition if this definition is placed
                // on the same line as another (outer) one as this way we'd pick
                // doc comment of the outer definition. For example (where field
                // of the struct would pick up struct's doc comment)
                //
                // /// Struct doc comment
                // public struct Tmp { field: u64 }
                return None;
            }
        }
    }

    if start_position.line_offset() == 0 {
        return None;
    }

    let mut iter = start_position.line_offset() - 1;
    let mut line_before = file_lines[iter].trim();

    let mut doc_string = String::new();
    // Detect the two different types of docstrings
    if line_before.starts_with("///") {
        while let Some(stripped_line) = line_before.strip_prefix("///") {
            doc_string = format!("{}\n{}", stripped_line.trim(), doc_string);
            if iter == 0 {
                break;
            }
            iter -= 1;
            line_before = file_lines[iter].trim();
        }
    } else if line_before.ends_with("*/") {
        let mut doc_string_found = false;
        line_before = file_lines[iter].strip_suffix("*/").unwrap_or("").trim();

        // Loop condition is a safe guard.
        while !doc_string_found {
            // We found the start of the multi-line comment/docstring
            if line_before.starts_with("/*") {
                let is_doc = line_before.starts_with("/**") && !line_before.starts_with("/***");

                // Invalid doc_string start prefix.
                if !is_doc {
                    return None;
                }

                line_before = line_before.strip_prefix("/**").unwrap_or("").trim();
                doc_string_found = true;
            }

            doc_string = format!("{}\n{}", line_before, doc_string);

            if iter == 0 {
                break;
            }

            iter -= 1;
            line_before = file_lines[iter].trim();
        }

        // No doc_string found - return String::new();
        if !doc_string_found {
            return None;
        }
    }

    // No point in trying to print empty comment
    if doc_string.is_empty() {
        return None;
    }

    Some(doc_string)
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
