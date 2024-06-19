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
    utils::loc_start_to_lsp_position_opt,
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
    naming::ast::{StructFields, Type, TypeName_, Type_},
    parser::ast::{self as P},
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

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Copy)]
/// Location of a definition's identifier
pub struct DefLoc {
    /// File where the definition of the identifier starts
    pub fhash: FileHash,
    /// Location where the definition of the identifier starts
    pub start: Position,
}

impl DefLoc {
    pub fn new(fhash: FileHash, start: Position) -> Self {
        Self { fhash, start }
    }

    pub fn fhash(&self) -> FileHash {
        self.fhash
    }

    pub fn start(&self) -> Position {
        self.start
    }
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
        Vec<Symbol>,
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
        Vec<Symbol>,
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
    def_loc: DefLoc,
    /// Location of the type definition
    type_def_loc: Option<DefLoc>,
}

/// Definition of a struct field
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FieldDef {
    pub name: Symbol,
    pub start: Position,
}

/// Definition of a struct
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct StructDef {
    pub name_start: Position,
    pub field_defs: Vec<FieldDef>,
    /// Does this struct have positional fields?
    pub positional: bool,
}

impl StructDef {
    pub fn name_start(&self) -> Position {
        self.name_start
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FunctionDef {
    pub name: Symbol,
    pub start: Position,
    pub attrs: Vec<String>,
}

/// Definition of a local (or parameter)
#[allow(clippy::non_canonical_partial_ord_impl)]
#[derive(Derivative, Debug, Clone, Eq, PartialEq)]
#[derivative(PartialOrd, Ord)]
pub struct LocalDef {
    /// Location of the definition
    pub def_loc: DefLoc,
    /// Type of definition
    #[derivative(PartialOrd = "ignore")]
    #[derivative(Ord = "ignore")]
    pub def_type: Type,
}

/// Definition of a constant
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConstDef {
    pub name_start: Position,
}

/// Module-level definitions
#[derive(Debug, Clone, Ord, PartialOrd, PartialEq, Eq)]
pub struct ModuleDefs {
    /// File where this module is located
    pub fhash: FileHash,
    /// Location where this module is located
    pub start: Position,
    /// Module name
    pub ident: ModuleIdent_,
    /// Struct definitions
    pub structs: BTreeMap<Symbol, StructDef>,
    /// Const definitions
    pub constants: BTreeMap<Symbol, ConstDef>,
    /// Function definitions
    pub functions: BTreeMap<Symbol, FunctionDef>,
    /// Definitions where the type is not explicitly specified
    pub untyped_defs: BTreeSet<DefLoc>,
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
    references: &'a mut BTreeMap<DefLoc, BTreeSet<UseLoc>>,
    /// Additional information about definitions
    def_info: &'a mut BTreeMap<DefLoc, DefInfo>,
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
}

/// Maps a line number to a list of use-def-s on a given line (use-def set is sorted by col_start)
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UseDefMap(BTreeMap<u32, BTreeSet<UseDef>>);

/// Result of the symbolication process
#[derive(Clone)]
pub struct Symbols {
    /// A map from def locations to all the references (uses)
    references: BTreeMap<DefLoc, BTreeSet<UseLoc>>,
    /// A mapping from uses to definitions in a file
    file_use_defs: BTreeMap<PathBuf, UseDefMap>,
    /// A mapping from filePath to ModuleDefs
    pub file_mods: BTreeMap<PathBuf, BTreeSet<ModuleDefs>>,
    /// Mapped file information for translating locations into positions
    pub files: MappedFiles,
    /// Additional information about definitions
    def_info: BTreeMap<DefLoc, DefInfo>,
    /// IDE Annotation Information from the Compiler
    pub compiler_info: CompilerInfo,
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
    pub fn functions(&self) -> &BTreeMap<Symbol, FunctionDef> {
        &self.functions
    }

    pub fn structs(&self) -> &BTreeMap<Symbol, StructDef> {
        &self.structs
    }

    pub fn fhash(&self) -> FileHash {
        self.fhash
    }

    pub fn untyped_defs(&self) -> &BTreeSet<DefLoc> {
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
                    struct_type_args_to_ide_string(type_args, /* verbose */ true);
                let abilities_str = if abilities.is_empty() {
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
                };
                // the mod_ident conversions below will ensure that only pkg name (without numerical
                // address) is displayed which is the same as in source
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
            Self::Field(mod_ident, struct_name, name, t, _) => {
                write!(
                    f,
                    "{}::{}\n{}: {}",
                    mod_ident,
                    struct_name,
                    name,
                    type_to_ide_string(t, /* verbose */ true)
                )
            }
            Self::Local(name, t, is_decl, is_mut) => {
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

fn struct_type_args_to_ide_string(type_args: &[(Type, bool)], verbose: bool) -> String {
    let mut type_args_str = "".to_string();
    if !type_args.is_empty() {
        type_args_str.push('<');
        type_args_str.push_str(&struct_type_list_to_ide_string(type_args, verbose));
        type_args_str.push('>');
    }
    type_args_str
}

fn typed_id_list_to_ide_string(
    names: &[Symbol],
    types: &[Type],
    separate_lines: bool,
    verbose: bool,
) -> String {
    names
        .iter()
        .zip(types.iter())
        .map(|(n, t)| {
            if separate_lines {
                format!("\t{}: {}", n, type_to_ide_string(t, verbose))
            } else {
                format!("{}: {}", n, type_to_ide_string(t, verbose))
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

fn struct_type_list_to_ide_string(types: &[(Type, bool)], verbose: bool) -> String {
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

impl SymbolicatorRunner {
    /// Create a new idle runner (one that does not actually symbolicate)
    pub fn idle() -> Self {
        let mtx_cvar = Arc::new((Mutex::new(RunnerState::Wait), Condvar::new()));
        SymbolicatorRunner { mtx_cvar }
    }

    /// Create a new runner
    pub fn new(
        ide_files_root: VfsPath,
        symbols: Arc<Mutex<Symbols>>,
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
                        match get_symbols(
                            pkg_deps.clone(),
                            ide_files_root.clone(),
                            root_dir.unwrap().as_path(),
                            lint,
                        ) {
                            Ok((symbols_opt, lsp_diagnostics)) => {
                                eprintln!("symbolication finished");
                                if let Some(new_symbols) = symbols_opt {
                                    // merge the new symbols with the old ones to support a
                                    // (potentially) new project/package that symbolication information
                                    // was built for
                                    //
                                    // TODO: we may consider "unloading" symbolication information when
                                    // files/directories are being closed but as with other performance
                                    // optimizations (e.g. incrementalizatino of the vfs), let's wait
                                    // until we know we actually need it
                                    let mut old_symbols = symbols.lock().unwrap();
                                    (*old_symbols).merge(new_symbols);
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
        references: &mut BTreeMap<DefLoc, BTreeSet<UseLoc>>,
        alias_lengths: &BTreeMap<Position, usize>,
        use_fhash: FileHash,
        use_start: Position,
        def_fhash: FileHash,
        def_start: Position,
        use_name: &Symbol,
        type_def_loc: Option<DefLoc>,
    ) -> Self {
        let def_loc = DefLoc::new(def_fhash, def_start);
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
        references: &mut BTreeMap<DefLoc, BTreeSet<UseLoc>>,
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

    pub fn def_loc(&self) -> DefLoc {
        self.def_loc
    }
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

    pub fn extend(&mut self, use_defs: BTreeMap<u32, BTreeSet<UseDef>>) {
        for (k, v) in use_defs {
            self.0.entry(k).or_default().extend(v);
        }
    }
}

impl Symbols {
    pub fn merge(&mut self, other: Self) {
        for (k, v) in other.references {
            self.references.entry(k).or_default().extend(v);
        }
        self.files.extend(other.files);
        self.def_info.extend(other.def_info);
    }

    pub fn line_uses(&self, use_fpath: &Path, use_line: u32) -> BTreeSet<UseDef> {
        let Some(file_symbols) = self.file_use_defs.get(use_fpath) else {
            return BTreeSet::new();
        };
        file_symbols.get(use_line).unwrap_or_else(BTreeSet::new)
    }

    pub fn def_info(&self, def_loc: &DefLoc) -> Option<&DefInfo> {
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
    let resolution_graph = build_config.resolution_graph_for_package(pkg_path, &mut Vec::new())?;
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
                mapped_files.extend(libs.files.clone());
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
        mapped_files.extend(compiler.compilation_env_ref().mapped_files().clone());

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
    let mut typed_modules = typed_ast.unwrap().modules;

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

    pre_process_typed_modules(
        &typed_modules,
        &mapped_files,
        &file_id_to_lines,
        &mut mod_outer_defs,
        &mut mod_use_defs,
        &mut references,
        &mut def_info,
        &edition,
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
    };

    parsing_symbolicator.prog_symbols(
        &parsed_program,
        &mut mod_use_defs,
        &mut mod_to_alias_lengths,
    );
    if let Some(libs) = compiled_libs.clone() {
        parsing_symbolicator.prog_symbols(
            &libs.parser,
            &mut mod_use_defs,
            &mut mod_to_alias_lengths,
        );
    }

    let mut compiler_info = compiler_info.unwrap();
    let mut typing_symbolicator = typing_analysis::TypingAnalysisContext {
        mod_outer_defs: &mod_outer_defs,
        files: &mapped_files,
        references: &mut references,
        def_info: &mut def_info,
        use_defs: UseDefMap::new(),
        alias_lengths: &BTreeMap::new(),
        traverse_only: false,
        compiler_info: &mut compiler_info,
        type_params: BTreeMap::new(),
        expression_scope: OrdMap::new(),
    };

    process_typed_modules(
        &mut typed_modules,
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

    let mut file_mods: BTreeMap<PathBuf, BTreeSet<ModuleDefs>> = BTreeMap::new();
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
    };

    eprintln!("get_symbols load complete");

    Ok((Some(symbols), ide_diagnostics))
}

fn pre_process_typed_modules(
    typed_modules: &UniqueMap<ModuleIdent, ModuleDefinition>,
    files: &MappedFiles,
    file_id_to_lines: &HashMap<usize, Vec<String>>,
    mod_outer_defs: &mut BTreeMap<String, ModuleDefs>,
    mod_use_defs: &mut BTreeMap<String, UseDefMap>,
    references: &mut BTreeMap<DefLoc, BTreeSet<UseLoc>>,
    def_info: &mut BTreeMap<DefLoc, DefInfo>,
    edition: &Option<Edition>,
) {
    for (pos, module_ident, module_def) in typed_modules {
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
    file_use_defs: &mut BTreeMap<PathBuf, UseDefMap>,
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
            .extend(use_defs.elements());
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

/// Get symbols for outer definitions in the module (functions, structs, and consts)
fn get_mod_outer_defs(
    loc: &Loc,
    mod_ident: &ModuleIdent,
    mod_def: &ModuleDefinition,
    files: &MappedFiles,
    file_id_to_lines: &HashMap<usize, Vec<String>>,
    references: &mut BTreeMap<DefLoc, BTreeSet<UseLoc>>,
    def_info: &mut BTreeMap<DefLoc, DefInfo>,
    edition: &Option<Edition>,
) -> (ModuleDefs, UseDefMap) {
    let mut structs = BTreeMap::new();
    let mut constants = BTreeMap::new();
    let mut functions = BTreeMap::new();

    let fhash = loc.file_hash();

    let mut positional = false;
    for (pos, name, def) in &mod_def.structs {
        // process struct fields first
        let mut field_defs = vec![];
        let mut field_types = vec![];
        if let StructFields::Defined(pos_fields, fields) = &def.fields {
            positional = *pos_fields;
            for (fpos, fname, (_, t)) in fields {
                let start = match loc_start_to_lsp_position_opt(files, &fpos) {
                    Some(s) => s,
                    None => {
                        debug_assert!(false);
                        continue;
                    }
                };
                field_defs.push(FieldDef {
                    name: *fname,
                    start,
                });
                let doc_string = extract_doc_string(
                    files.file_mapping(),
                    file_id_to_lines,
                    &start,
                    &fpos.file_hash(),
                );
                def_info.insert(
                    DefLoc::new(fhash, start),
                    DefInfo::Field(mod_ident.value, *name, *fname, t.clone(), doc_string),
                );
                field_types.push(t.clone());
            }
        };

        // process the struct itself
        let name_start = match loc_start_to_lsp_position_opt(files, &pos) {
            Some(s) => s,
            None => {
                debug_assert!(false);
                continue;
            }
        };

        let field_names = field_defs.iter().map(|f| f.name).collect();
        structs.insert(
            *name,
            StructDef {
                name_start,
                field_defs,
                positional,
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
        let doc_string = extract_doc_string(
            files.file_mapping(),
            file_id_to_lines,
            &name_start,
            &pos.file_hash(),
        );
        def_info.insert(
            DefLoc::new(fhash, name_start),
            DefInfo::Struct(
                mod_ident.value,
                *name,
                visibility,
                def.type_parameters
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
                    .collect(),
                def.abilities.clone(),
                field_names,
                field_types,
                doc_string,
            ),
        );
    }

    for (pos, name, c) in &mod_def.constants {
        let name_start = match loc_start_to_lsp_position_opt(files, &pos) {
            Some(s) => s,
            None => {
                debug_assert!(false);
                continue;
            }
        };
        constants.insert(*name, ConstDef { name_start });
        let doc_string = extract_doc_string(
            files.file_mapping(),
            file_id_to_lines,
            &name_start,
            &pos.file_hash(),
        );
        def_info.insert(
            DefLoc::new(fhash, name_start),
            DefInfo::Const(
                mod_ident.value,
                *name,
                c.signature.clone(),
                const_val_to_ide_string(&c.value),
                doc_string,
            ),
        );
    }

    for (pos, name, fun) in &mod_def.functions {
        if ignored_function(*name) {
            continue;
        }
        let name_start = match loc_start_to_lsp_position_opt(files, &pos) {
            Some(s) => s,
            None => {
                debug_assert!(false);
                continue;
            }
        };
        let fun_type = if fun.entry.is_some() {
            FunType::Entry
        } else if fun.macro_.is_some() {
            FunType::Macro
        } else {
            FunType::Regular
        };
        let doc_string = extract_doc_string(
            files.file_mapping(),
            file_id_to_lines,
            &name_start,
            &pos.file_hash(),
        );
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
                .map(|(_, n, _)| n.value.name)
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
            FunctionDef {
                name: *name,
                start: name_start,
                attrs: fun
                    .attributes
                    .clone()
                    .iter()
                    .map(|(_loc, name, _attr)| name.to_string())
                    .collect(),
            },
        );
        def_info.insert(DefLoc::new(loc.file_hash(), name_start), fun_info);
    }

    let mut use_def_map = UseDefMap::new();

    let ident = mod_ident.value;
    let start = match loc_start_to_lsp_position_opt(files, loc) {
        Some(s) => s,
        None => {
            debug_assert!(false);
            return (
                ModuleDefs {
                    fhash,
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    ident,
                    structs,
                    constants,
                    functions,
                    untyped_defs: BTreeSet::new(),
                },
                use_def_map,
            );
        }
    };

    let doc_comment = extract_doc_string(files.file_mapping(), file_id_to_lines, &start, &fhash);
    let mod_defs = ModuleDefs {
        fhash,
        ident,
        start,
        structs,
        constants,
        functions,
        untyped_defs: BTreeSet::new(),
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
                mod_defs.fhash,
                mod_defs.start,
                &mod_name.value(),
                None,
            ),
        );
        def_info.insert(
            DefLoc::new(mod_defs.fhash, mod_defs.start),
            DefInfo::Module(mod_ident_to_ide_string(&ident), doc_comment),
        );
    }

    (mod_defs, use_def_map)
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
                    fun.signature
                        .parameters
                        .iter()
                        .for_each(|(_, _, t)| self.type_symbols(t));
                    self.type_symbols(&fun.signature.return_type);
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
                MM::Struct(sdef) => match &sdef.fields {
                    P::StructFields::Named(v) => v.iter().for_each(|(_, t)| self.type_symbols(t)),
                    P::StructFields::Positional(v) => v.iter().for_each(|t| self.type_symbols(t)),
                    P::StructFields::Native(_) => (),
                },
                MM::Enum(edef) => {
                    let P::EnumDefinition { variants, .. } = edef;
                    for variant in variants {
                        let P::VariantDefinition { fields, .. } = variant;
                        match fields {
                            P::VariantFields::Named(v) => {
                                v.iter().for_each(|(_, t)| self.type_symbols(t))
                            }
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
    fn exp_symbols(&mut self, sp!(_, exp): &P::Exp) {
        use P::Exp_ as E;
        match exp {
            E::Move(_, e) => self.exp_symbols(e),
            E::Copy(_, e) => self.exp_symbols(e),
            E::Name(chain) => self.chain_symbols(chain),
            E::Call(chain, v) => {
                self.chain_symbols(chain);
                v.value.iter().for_each(|e| self.exp_symbols(e));
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
                    v.iter().for_each(|bind| self.bind_symbols(bind, to.is_some()));
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
            E::DotCall(e, _, _, vo, v) => {
                self.exp_symbols(e);
                if let Some(v) = vo {
                    v.iter().for_each(|t| self.type_symbols(t));
                }
                v.value.iter().for_each(|e| self.exp_symbols(e));
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
            | E::Match(_, _) // TODO support it
            | E::UnresolvedError
            => (),
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
                mod_defs.fhash,
                mod_defs.start,
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
        if let Some(mut ud) = add_struct_use_def(
            self.mod_outer_defs,
            self.files,
            mod_ident_str.clone(),
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
        if let Some(mut ud) = add_fun_use_def(
            &name.value,
            self.mod_outer_defs,
            self.files,
            mod_ident_str.clone(),
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
    fn type_symbols(&mut self, sp!(_, t): &P::Type) {
        use P::Type_ as T;
        match t {
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
    fn bind_symbols(&mut self, sp!(_, bind): &P::Bind, explicitly_typed: bool) {
        use P::Bind_ as B;
        match bind {
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
                    let Some(mod_defs) = self
                        .mod_outer_defs
                        .get_mut(&self.current_mod_ident_str.clone().unwrap())
                    else {
                        return;
                    };
                    let Some(def_start) = loc_start_to_lsp_position_opt(self.files, &var.loc())
                    else {
                        return;
                    };
                    mod_defs
                        .untyped_defs
                        .insert(DefLoc::new(var.loc().file_hash(), def_start));
                }
            }
        }
    }

    /// Get symbols for a name access chain
    fn chain_symbols(&mut self, sp!(_, chain): &P::NameAccessChain) {
        use P::NameAccessChain_ as NA;
        // record the length of an identifier representing a potentially
        // aliased module, struct or function  name in an access chain,
        let no = match chain {
            NA::Single(entry) => {
                self.path_entry_symbols(entry);
                Some(entry.name)
            }
            NA::Path(path) => {
                let P::NamePath { root, entries } = path;
                self.root_path_entry_symbols(root);
                entries
                    .iter()
                    .for_each(|entry| self.path_entry_symbols(entry));
                // FIXME: this is a hack that will break when we add enums
                if entries.len() < 2 {
                    if let P::LeadingNameAccess_::Name(n) = root.name.value {
                        Some(n)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        };
        let Some(n) = no else {
            return;
        };
        let sp!(pos, name) = n;
        let Some(loc) = loc_start_to_lsp_position_opt(self.files, &pos) else {
            return;
        };
        self.alias_lengths.insert(loc, name.len());
    }
}

/// Add use of a function identifier
pub fn add_fun_use_def(
    fun_def_name: &Symbol, // may be different from use_name for methods
    mod_outer_defs: &BTreeMap<String, ModuleDefs>,
    files: &MappedFiles,
    mod_ident_str: String,
    mod_defs: &ModuleDefs,
    use_name: &Symbol,
    use_pos: &Loc,
    references: &mut BTreeMap<DefLoc, BTreeSet<UseLoc>>,
    def_info: &BTreeMap<DefLoc, DefInfo>,
    use_defs: &mut UseDefMap,
    alias_lengths: &BTreeMap<Position, usize>,
) -> Option<UseDef> {
    let Some(name_start) = loc_start_to_lsp_position_opt(files, use_pos) else {
        debug_assert!(false);
        return None;
    };
    if let Some(func_def) = mod_defs.functions.get(fun_def_name) {
        let def_fhash = mod_outer_defs.get(&mod_ident_str).unwrap().fhash;
        let fun_info = def_info
            .get(&DefLoc::new(def_fhash, func_def.start))
            .unwrap();
        let ident_type_def_loc = def_info_to_type_def_loc(mod_outer_defs, fun_info);
        let ud = UseDef::new(
            references,
            alias_lengths,
            use_pos.file_hash(),
            name_start,
            def_fhash,
            func_def.start,
            use_name,
            ident_type_def_loc,
        );
        use_defs.insert(name_start.line, ud.clone());
        return Some(ud);
    }
    None
}

/// Add use of a struct identifier
pub fn add_struct_use_def(
    mod_outer_defs: &BTreeMap<String, ModuleDefs>,
    files: &MappedFiles,
    mod_ident_str: String,
    mod_defs: &ModuleDefs,
    use_name: &Symbol,
    use_pos: &Loc,
    references: &mut BTreeMap<DefLoc, BTreeSet<UseLoc>>,
    def_info: &BTreeMap<DefLoc, DefInfo>,
    use_defs: &mut UseDefMap,
    alias_lengths: &BTreeMap<Position, usize>,
) -> Option<UseDef> {
    let Some(name_start) = loc_start_to_lsp_position_opt(files, use_pos) else {
        debug_assert!(false);
        return None;
    };
    if let Some(def) = mod_defs.structs.get(use_name) {
        let def_fhash = mod_outer_defs.get(&mod_ident_str).unwrap().fhash;
        let struct_info = def_info
            .get(&DefLoc::new(def_fhash, def.name_start))
            .unwrap();
        let ident_type_def_loc = def_info_to_type_def_loc(mod_outer_defs, struct_info);
        let ud = UseDef::new(
            references,
            alias_lengths,
            use_pos.file_hash(),
            name_start,
            def_fhash,
            def.name_start,
            use_name,
            ident_type_def_loc,
        );
        use_defs.insert(name_start.line, ud.clone());
        return Some(ud);
    }
    None
}

pub fn def_info_to_type_def_loc(
    mod_outer_defs: &BTreeMap<String, ModuleDefs>,
    def_info: &DefInfo,
) -> Option<DefLoc> {
    match def_info {
        DefInfo::Type(t) => type_def_loc(mod_outer_defs, t),
        DefInfo::Function(..) => None,
        DefInfo::Struct(mod_ident, name, ..) => find_struct(mod_outer_defs, mod_ident, name),
        DefInfo::Field(.., t, _) => type_def_loc(mod_outer_defs, t),
        DefInfo::Local(_, t, _, _) => type_def_loc(mod_outer_defs, t),
        DefInfo::Const(_, _, t, _, _) => type_def_loc(mod_outer_defs, t),
        DefInfo::Module(..) => None,
    }
}

fn def_info_doc_string(def_info: &DefInfo) -> Option<String> {
    match def_info {
        DefInfo::Type(_) => None,
        DefInfo::Function(.., s) => s.clone(),
        DefInfo::Struct(.., s) => s.clone(),
        DefInfo::Field(.., s) => s.clone(),
        DefInfo::Local(..) => None,
        DefInfo::Const(.., s) => s.clone(),
        DefInfo::Module(_, s) => s.clone(),
    }
}

pub fn type_def_loc(
    mod_outer_defs: &BTreeMap<String, ModuleDefs>,
    sp!(_, t): &Type,
) -> Option<DefLoc> {
    match t {
        Type_::Ref(_, r) => type_def_loc(mod_outer_defs, r),
        Type_::Apply(_, sp!(_, TypeName_::ModuleType(sp!(_, mod_ident), struct_name)), _) => {
            find_struct(mod_outer_defs, mod_ident, &struct_name.value())
        }
        _ => None,
    }
}

fn find_struct(
    mod_outer_defs: &BTreeMap<String, ModuleDefs>,
    mod_ident: &ModuleIdent_,
    struct_name: &Symbol,
) -> Option<DefLoc> {
    let mod_ident_str = expansion_mod_ident_to_map_key(mod_ident);
    let mod_defs = match mod_outer_defs.get(&mod_ident_str) {
        Some(v) => v,
        None => return None,
    };
    mod_defs.structs.get(struct_name).map(|struct_def| {
        let fhash = mod_defs.fhash;
        let start = struct_def.name_start;
        DefLoc::new(fhash, start)
    })
}

/// Extracts the docstring (/// or /** ... */) for a given definition by traversing up from the line definition
fn extract_doc_string(
    file_id_mapping: &HashMap<FileHash, usize>,
    file_id_to_lines: &HashMap<usize, Vec<String>>,
    name_start: &Position,
    file_hash: &FileHash,
) -> Option<String> {
    let Some(file_id) = file_id_mapping.get(file_hash) else {
        return None;
    };

    let Some(file_lines) = file_id_to_lines.get(file_id) else {
        return None;
    };

    if name_start.line == 0 {
        return None;
    }

    let mut iter = (name_start.line - 1) as usize;
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
pub fn on_go_to_def_request(context: &Context, request: &Request, symbols: &Symbols) {
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
        symbols,
        &fpath,
        line,
        col,
        request.id.clone(),
        |u| {
            let loc = def_ide_location(&u.def_loc, symbols);
            Some(serde_json::to_value(loc).unwrap())
        },
    );
}

pub fn def_ide_location(def_loc: &DefLoc, symbols: &Symbols) -> Location {
    // TODO: Do we need beginning and end of the definition? Does not seem to make a
    // difference from the IDE perspective as the cursor goes to the beginning anyway (at
    // least in VSCode).
    let range = Range {
        start: def_loc.start,
        end: def_loc.start,
    };
    let path = symbols.files.file_path(&def_loc.fhash);
    Location {
        uri: Url::from_file_path(path).unwrap(),
        range,
    }
}

/// Handles go-to-type-def request of the language server
pub fn on_go_to_type_def_request(context: &Context, request: &Request, symbols: &Symbols) {
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
        symbols,
        &fpath,
        line,
        col,
        request.id.clone(),
        |u| match u.type_def_loc {
            Some(def_loc) => {
                let range = Range {
                    start: def_loc.start,
                    end: def_loc.start,
                };
                let path = symbols.files.file_path(&u.def_loc.fhash);
                let loc = Location {
                    uri: Url::from_file_path(path).unwrap(),
                    range,
                };
                Some(serde_json::to_value(loc).unwrap())
            }
            None => Some(serde_json::to_value(Option::<lsp_types::Location>::None).unwrap()),
        },
    );
}

/// Handles go-to-references request of the language server
pub fn on_references_request(context: &Context, request: &Request, symbols: &Symbols) {
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
        symbols,
        &fpath,
        line,
        col,
        request.id.clone(),
        |u| match symbols.references.get(&u.def_loc) {
            Some(s) => {
                let mut locs = vec![];
                for ref_loc in s {
                    if include_decl
                        || !(u.def_loc.start == ref_loc.start && u.def_loc.fhash == ref_loc.fhash)
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
                if locs.is_empty() {
                    Some(serde_json::to_value(Option::<lsp_types::Location>::None).unwrap())
                } else {
                    Some(serde_json::to_value(locs).unwrap())
                }
            }
            None => Some(serde_json::to_value(Option::<lsp_types::Location>::None).unwrap()),
        },
    );
}

/// Handles hover request of the language server
pub fn on_hover_request(context: &Context, request: &Request, symbols: &Symbols) {
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
        symbols,
        &fpath,
        line,
        col,
        request.id.clone(),
        |u| {
            let Some(info) = symbols.def_info.get(&u.def_loc) else {
                return Some(serde_json::to_value(Option::<lsp_types::Location>::None).unwrap());
            };
            // use rust for highlighting in Markdown until there is support for Move
            let contents = HoverContents::Markup(on_hover_markup(info));
            let range = None;
            Some(serde_json::to_value(Hover { contents, range }).unwrap())
        },
    );
}

pub fn on_hover_markup(info: &DefInfo) -> MarkupContent {
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
    symbols: &Symbols,
    use_fpath: &PathBuf,
    use_line: u32,
    use_col: u32,
    id: RequestId,
    use_def_action: impl Fn(&UseDef) -> Option<serde_json::Value>,
) {
    let mut result = None;

    let mut use_def_found = false;
    if let Some(mod_symbols) = symbols.file_use_defs.get(use_fpath) {
        if let Some(uses) = mod_symbols.get(use_line) {
            for u in uses {
                if use_col >= u.col_start && use_col <= u.col_end {
                    result = use_def_action(&u);
                    use_def_found = true;
                }
            }
        }
    }
    if !use_def_found {
        result = Some(serde_json::to_value(Option::<lsp_types::Location>::None).unwrap());
    }

    eprintln!("about to send use response");
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
pub fn on_document_symbol_request(context: &Context, request: &Request, symbols: &Symbols) {
    let parameters = serde_json::from_value::<DocumentSymbolParams>(request.params.clone())
        .expect("could not deserialize document symbol request");

    let fpath = parameters.text_document.uri.to_file_path().unwrap();
    eprintln!("on_document_symbol_request: {:?}", fpath);

    let empty_mods: BTreeSet<ModuleDefs> = BTreeSet::new();
    let mods = symbols.file_mods.get(&fpath).unwrap_or(&empty_mods);

    let mut defs: Vec<DocumentSymbol> = vec![];
    for mod_def in mods {
        let name = mod_def.ident.module.clone().to_string();
        let detail = Some(mod_def.ident.clone().to_string());
        let kind = SymbolKind::MODULE;
        let range = Range {
            start: mod_def.start,
            end: mod_def.start,
        };

        let mut children = vec![];

        // handle constants
        let cloned_const_def = mod_def.constants.clone();
        for (sym, const_def) in cloned_const_def {
            let const_range = Range {
                start: const_def.name_start,
                end: const_def.name_start,
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
        let cloned_struct_def = mod_def.structs.clone();
        for (sym, struct_def) in cloned_struct_def {
            let struct_range = Range {
                start: struct_def.name_start,
                end: struct_def.name_start,
            };

            let mut fields: Vec<DocumentSymbol> = vec![];
            handle_struct_fields(struct_def, &mut fields);

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

        // handle functions
        let cloned_func_def = mod_def.functions.clone();
        for (sym, func_def) in cloned_func_def {
            let func_range = Range {
                start: func_def.start,
                end: func_def.start,
            };

            let mut detail = None;
            if !func_def.attrs.is_empty() {
                detail = Some(format!("{:?}", func_def.attrs));
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

/// Helper function to handle struct fields
#[allow(deprecated)]
fn handle_struct_fields(struct_def: StructDef, fields: &mut Vec<DocumentSymbol>) {
    let clonded_fileds = struct_def.field_defs;

    for field_def in clonded_fileds {
        let field_range = Range {
            start: field_def.start,
            end: field_def.start,
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

#[cfg(test)]
fn assert_use_def_with_doc_string(
    mod_symbols: &UseDefMap,
    symbols: &Symbols,
    use_idx: usize,
    use_line: u32,
    use_col: u32,
    use_file: &str,
    def_line: u32,
    def_col: u32,
    def_file: &str,
    type_str: &str,
    type_def: Option<(u32, u32, &str)>,
    doc_string: Option<&str>,
) {
    let file_name_mapping = &symbols.files.file_name_mapping();
    let def_info = &symbols.def_info;

    let Some(uses) = mod_symbols.get(use_line) else {
        panic!("No use_line {use_line} in mod_symbols {mod_symbols:#?} for file {use_file}");
    };
    let Some(use_def) = uses.iter().nth(use_idx) else {
        panic!("No use_line {use_idx} in uses {uses:#?} for file {use_file}");
    };
    assert!(
        use_def.col_start == use_col,
        "'{}' != '{}' for use in column {use_col} of line {use_line} in file {use_file}",
        use_def.col_start,
        use_col,
    );
    assert!(
        use_def.def_loc.start.line == def_line,
        "'{}' != '{}' for use in column {use_col} of line {use_line} in file {use_file}",
        use_def.def_loc.start.line,
        def_line
    );
    assert!(
        use_def.def_loc.start.character == def_col,
        "'{}' != '{}' for use in column {use_col} of line {use_line} in file {use_file}",
        use_def.def_loc.start.character,
        def_col
    );
    assert!(
        file_name_mapping
            .get(&use_def.def_loc.fhash)
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with(def_file),
        "for use in column {use_col} of line {use_line} in file {use_file}"
    );
    let info = def_info.get(&use_def.def_loc).unwrap();
    let info_str = info.to_string();
    assert!(
        type_str == info_str,
        "'{type_str}' != '{info}' for use in column {use_col} of line {use_line} in file {use_file}",
    );

    if doc_string.is_some() {
        let expected_doc_string = def_info_doc_string(info);
        assert!(
            doc_string.map(|s| s.to_string()) == expected_doc_string,
            "'{:?}' != '{:?}' for use in column {use_col} of line {use_line} in file {use_file}",
            doc_string.map(|s| s.to_string()),
            expected_doc_string
        );
    }
    match use_def.type_def_loc {
        Some(type_def_loc) => {
            let tdef_line = type_def.unwrap().0;
            let tdef_col = type_def.unwrap().1;
            let tdef_file = type_def.unwrap().2;
            assert!(
                type_def_loc.start.line == tdef_line,
                "'{}' != '{}' for use in column {use_col} of line {use_line} in file {use_file}",
                type_def_loc.start.line,
                tdef_line
            );
            assert!(
                type_def_loc.start.character == tdef_col,
                "'{}' != '{}' for use in column {use_col} of line {use_line} in file {use_file}",
                type_def_loc.start.character,
                tdef_col
            );
            assert!(
                file_name_mapping
                    .get(&type_def_loc.fhash)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .ends_with(tdef_file),
                "for use in column {use_col} of line {use_line} in file {use_file}"
            );
        }
        None => assert!(
            type_def.is_none(),
            "for use in column {use_col} of line {use_line} in file {use_file}"
        ),
    }
}

#[cfg(test)]
fn assert_use_def(
    mod_symbols: &UseDefMap,
    symbols: &Symbols,
    use_idx: usize,
    use_line: u32,
    use_col: u32,
    use_file: &str,
    def_line: u32,
    def_col: u32,
    def_file: &str,
    type_str: &str,
    type_def: Option<(u32, u32, &str)>,
) {
    assert_use_def_with_doc_string(
        mod_symbols,
        symbols,
        use_idx,
        use_line,
        use_col,
        use_file,
        def_line,
        def_col,
        def_file,
        type_str,
        type_def,
        None,
    )
}

#[test]
/// Tests if symbolication + doc_string information for documented Move constructs is constructed correctly.
fn docstring_test() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    path.push("tests/symbols");

    let ide_files_layer: VfsPath = MemoryFS::new().into();
    let (symbols_opt, _) = get_symbols(
        Arc::new(Mutex::new(BTreeMap::new())),
        ide_files_layer,
        path.as_path(),
        LintLevel::None,
    )
    .unwrap();
    let symbols = symbols_opt.unwrap();

    let mut fpath = path.clone();
    fpath.push("sources/M6.move");
    let cpath = dunce::canonicalize(&fpath).unwrap();

    let mod_symbols = symbols.file_use_defs.get(&cpath).unwrap();

    // struct def name
    assert_use_def_with_doc_string(
        mod_symbols,
        &symbols,
        0,
        4,
        11,
        "M6.move",
        4,
        11,
        "M6.move",
        "struct Symbols::M6::DocumentedStruct has drop, store, key {\n\tdocumented_field: u64\n}",
        Some((4, 11, "M6.move")),
        Some("This is a documented struct\nWith a multi-line docstring\n"),
    );

    // const def name
    assert_use_def_with_doc_string(
        mod_symbols,
        &symbols,
        0,
        10,
        10,
        "M6.move",
        10,
        10,
        "M6.move",
        "const Symbols::M6::DOCUMENTED_CONSTANT: u64 = 42",
        None,
        Some("Constant containing the answer to the universe\n"),
    );

    // function def name
    assert_use_def_with_doc_string(
        mod_symbols,
        &symbols,
        0,
        14,
        8,
        "M6.move",
        14,
        8,
        "M6.move",
        "fun Symbols::M6::unpack(s: Symbols::M6::DocumentedStruct): u64",
        None,
        Some("A documented function that unpacks a DocumentedStruct\n"),
    );
    // param var (unpack function) - should not have doc string
    assert_use_def_with_doc_string(
        mod_symbols,
        &symbols,
        1,
        14,
        15,
        "M6.move",
        14,
        15,
        "M6.move",
        "s: Symbols::M6::DocumentedStruct",
        Some((4, 11, "M6.move")),
        None,
    );
    // struct name in param type (unpack function)
    assert_use_def_with_doc_string(
        mod_symbols,
        &symbols,
        2,
        14,
        18,
        "M6.move",
        4,
        11,
        "M6.move",
        "struct Symbols::M6::DocumentedStruct has drop, store, key {\n\tdocumented_field: u64\n}",
        Some((4, 11, "M6.move")),
        Some("This is a documented struct\nWith a multi-line docstring\n"),
    );
    // struct name in unpack (unpack function)
    assert_use_def_with_doc_string(
        mod_symbols,
        &symbols,
        0,
        15,
        12,
        "M6.move",
        4,
        11,
        "M6.move",
        "struct Symbols::M6::DocumentedStruct has drop, store, key {\n\tdocumented_field: u64\n}",
        Some((4, 11, "M6.move")),
        Some("This is a documented struct\nWith a multi-line docstring\n"),
    );
    // field name in unpack (unpack function)
    assert_use_def_with_doc_string(
        mod_symbols,
        &symbols,
        1,
        15,
        31,
        "M6.move",
        6,
        8,
        "M6.move",
        "Symbols::M6::DocumentedStruct\ndocumented_field: u64",
        None,
        Some("A documented field\n"),
    );
    // moved var in unpack assignment (unpack function)
    assert_use_def_with_doc_string(
        mod_symbols,
        &symbols,
        3,
        15,
        59,
        "M6.move",
        14,
        15,
        "M6.move",
        "s: Symbols::M6::DocumentedStruct",
        Some((4, 11, "M6.move")),
        None,
    );

    // docstring construction for multi-line /** .. */ based strings
    assert_use_def_with_doc_string(
        mod_symbols,
        &symbols,
        0,
        26,
        8,
        "M6.move",
        26,
        8,
        "M6.move",
        "fun Symbols::M6::other_doc_struct(): Symbols::M7::OtherDocStruct",
        None,
        Some("\nThis is a multiline docstring\n\nThis docstring has empty lines.\n\nIt uses the ** format instead of ///\n\n"),
    );

    // docstring construction for single-line /** .. */ based strings
    assert_use_def_with_doc_string(
        mod_symbols,
        &symbols,
        0,
        31,
        8,
        "M6.move",
        31,
        8,
        "M6.move",
        "fun Symbols::M6::acq(uint: u64): u64",
        None,
        Some("Asterix based single-line docstring\n"),
    );

    /* Test doc_string construction for struct/function imported from another module */

    // other module struct name (other_doc_struct function)
    assert_use_def_with_doc_string(
        mod_symbols,
        &symbols,
        2,
        26,
        41,
        "M6.move",
        3,
        11,
        "M7.move",
        "struct Symbols::M7::OtherDocStruct has drop {\n\tsome_field: u64\n}",
        Some((3, 11, "M7.move")),
        Some("Documented struct in another module\n"),
    );

    // function name in a call (other_doc_struct function)
    assert_use_def_with_doc_string(
        mod_symbols,
        &symbols,
        1,
        27,
        21,
        "M6.move",
        9,
        15,
        "M7.move",
        "public fun Symbols::M7::create_other_struct(v: u64): Symbols::M7::OtherDocStruct",
        None,
        Some("Documented initializer in another module\n"),
    );

    // const in param (other_doc_struct function)
    assert_use_def_with_doc_string(
        mod_symbols,
        &symbols,
        2,
        27,
        41,
        "M6.move",
        10,
        10,
        "M6.move",
        "const Symbols::M6::DOCUMENTED_CONSTANT: u64 = 42",
        None,
        Some("Constant containing the answer to the universe\n"),
    );

    // other documented struct name imported (other_doc_struct_import function)
    assert_use_def_with_doc_string(
        mod_symbols,
        &symbols,
        1,
        38,
        35,
        "M6.move",
        3,
        11,
        "M7.move",
        "struct Symbols::M7::OtherDocStruct has drop {\n\tsome_field: u64\n}",
        Some((3, 11, "M7.move")),
        Some("Documented struct in another module\n"),
    );

    // Type param definition in documented function (type_param_doc function) - should have no doc string
    assert_use_def_with_doc_string(
        mod_symbols,
        &symbols,
        1,
        43,
        23,
        "M6.move",
        43,
        23,
        "M6.move",
        "T",
        None,
        None,
    );

    // Param def (of generic type) in documented function (type_param_doc function) - should have no doc string
    assert_use_def_with_doc_string(
        mod_symbols,
        &symbols,
        2,
        43,
        39,
        "M6.move",
        43,
        39,
        "M6.move",
        "param: T",
        None,
        None,
    );
}

#[test]
/// Tests if symbolication information for specific Move constructs has been constructed correctly.
fn symbols_test() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    path.push("tests/symbols");

    let ide_files_layer: VfsPath = MemoryFS::new().into();
    let (symbols_opt, _) = get_symbols(
        Arc::new(Mutex::new(BTreeMap::new())),
        ide_files_layer,
        path.as_path(),
        LintLevel::None,
    )
    .unwrap();
    let symbols = symbols_opt.unwrap();

    let mut fpath = path.clone();
    fpath.push("sources/M1.move");
    let cpath = dunce::canonicalize(&fpath).unwrap();

    let mod_symbols = symbols.file_use_defs.get(&cpath).unwrap();

    // struct def name
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        2,
        11,
        "M1.move",
        2,
        11,
        "M1.move",
        "struct Symbols::M1::SomeStruct has drop, store, key {\n\tsome_field: u64\n}",
        Some((2, 11, "M1.move")),
    );
    // const def name
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        6,
        10,
        "M1.move",
        6,
        10,
        "M1.move",
        "const Symbols::M1::SOME_CONST: u64 = 42",
        None,
    );
    // function def name
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        9,
        8,
        "M1.move",
        9,
        8,
        "M1.move",
        "fun Symbols::M1::unpack(s: Symbols::M1::SomeStruct): u64",
        None,
    );
    // param var (unpack function)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        9,
        15,
        "M1.move",
        9,
        15,
        "M1.move",
        "s: Symbols::M1::SomeStruct",
        Some((2, 11, "M1.move")),
    );
    // struct name in param type (unpack function)
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        9,
        18,
        "M1.move",
        2,
        11,
        "M1.move",
        "struct Symbols::M1::SomeStruct has drop, store, key {\n\tsome_field: u64\n}",
        Some((2, 11, "M1.move")),
    );
    // struct name in unpack (unpack function)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        10,
        12,
        "M1.move",
        2,
        11,
        "M1.move",
        "struct Symbols::M1::SomeStruct has drop, store, key {\n\tsome_field: u64\n}",
        Some((2, 11, "M1.move")),
    );
    // field name in unpack (unpack function)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        10,
        25,
        "M1.move",
        3,
        8,
        "M1.move",
        "Symbols::M1::SomeStruct\nsome_field: u64",
        None,
    );
    // bound variable in unpack (unpack function)
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        10,
        37,
        "M1.move",
        10,
        37,
        "M1.move",
        "value: u64",
        None,
    );
    // moved var in unpack assignment (unpack function)
    assert_use_def(
        mod_symbols,
        &symbols,
        3,
        10,
        47,
        "M1.move",
        9,
        15,
        "M1.move",
        "s: Symbols::M1::SomeStruct",
        Some((2, 11, "M1.move")),
    );
    // copied var in an assignment (cp function)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        15,
        18,
        "M1.move",
        14,
        11,
        "M1.move",
        "value: u64",
        None,
    );
    // struct name return type (pack function)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        19,
        16,
        "M1.move",
        2,
        11,
        "M1.move",
        "struct Symbols::M1::SomeStruct has drop, store, key {\n\tsome_field: u64\n}",
        Some((2, 11, "M1.move")),
    );
    // struct name in pack (pack function)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        20,
        18,
        "M1.move",
        2,
        11,
        "M1.move",
        "struct Symbols::M1::SomeStruct has drop, store, key {\n\tsome_field: u64\n}",
        Some((2, 11, "M1.move")),
    );
    // field name in pack (pack function)
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        20,
        31,
        "M1.move",
        3,
        8,
        "M1.move",
        "Symbols::M1::SomeStruct\nsome_field: u64",
        None,
    );
    // const in pack (pack function)
    assert_use_def(
        mod_symbols,
        &symbols,
        3,
        20,
        43,
        "M1.move",
        6,
        10,
        "M1.move",
        "const Symbols::M1::SOME_CONST: u64 = 42",
        None,
    );
    // other module struct name (other_mod_struct function)
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        24,
        41,
        "M1.move",
        2,
        11,
        "M2.move",
        "struct Symbols::M2::SomeOtherStruct has drop {\n\tsome_field: u64\n}",
        Some((2, 11, "M2.move")),
    );
    // function name in a call (other_mod_struct function)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        25,
        21,
        "M1.move",
        6,
        15,
        "M2.move",
        "public fun Symbols::M2::some_other_struct(v: u64): Symbols::M2::SomeOtherStruct",
        None,
    );
    // const in param (other_mod_struct function)
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        25,
        39,
        "M1.move",
        6,
        10,
        "M1.move",
        "const Symbols::M1::SOME_CONST: u64 = 42",
        None,
    );
    // other module struct name imported (other_mod_struct_import function)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        30,
        35,
        "M1.move",
        2,
        11,
        "M2.move",
        "struct Symbols::M2::SomeOtherStruct has drop {\n\tsome_field: u64\n}",
        Some((2, 11, "M2.move")),
    );
    // function name (acq function)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        34,
        8,
        "M1.move",
        34,
        8,
        "M1.move",
        "fun Symbols::M1::acq(uint: u64): u64",
        None,
    );
    // const in first param (multi_arg_call function)
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        40,
        22,
        "M1.move",
        6,
        10,
        "M1.move",
        "const Symbols::M1::SOME_CONST: u64 = 42",
        None,
    );
    // const in second param (multi_arg_call function)
    assert_use_def(
        mod_symbols,
        &symbols,
        3,
        40,
        34,
        "M1.move",
        6,
        10,
        "M1.move",
        "const Symbols::M1::SOME_CONST: u64 = 42",
        None,
    );
    // function name (vec function)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        43,
        8,
        "M1.move",
        43,
        8,
        "M1.move",
        "fun Symbols::M1::vec(): vector<Symbols::M1::SomeStruct>",
        None,
    );
    // vector constructor type (vec function)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        45,
        15,
        "M1.move",
        2,
        11,
        "M1.move",
        "struct Symbols::M1::SomeStruct has drop, store, key {\n\tsome_field: u64\n}",
        Some((2, 11, "M1.move")),
    );
    // vector constructor first element struct type (vec function)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        45,
        27,
        "M1.move",
        2,
        11,
        "M1.move",
        "struct Symbols::M1::SomeStruct has drop, store, key {\n\tsome_field: u64\n}",
        Some((2, 11, "M1.move")),
    );
    // vector constructor first element struct field (vec function)
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        45,
        39,
        "M1.move",
        3,
        8,
        "M1.move",
        "Symbols::M1::SomeStruct\nsome_field: u64",
        None,
    );
    // vector constructor second element var (vec function)
    assert_use_def(
        mod_symbols,
        &symbols,
        3,
        45,
        57,
        "M1.move",
        44,
        12,
        "M1.move",
        "let s: Symbols::M1::SomeStruct",
        Some((2, 11, "M1.move")),
    );
    // borrow local (mut function)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        56,
        21,
        "M1.move",
        55,
        12,
        "M1.move",
        "let tmp: u64",
        None,
    );
    // LHS in mutation statement (mut function)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        57,
        9,
        "M1.move",
        56,
        12,
        "M1.move",
        "let r: &mut u64",
        None,
    );
    // RHS in mutation statement (mut function)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        57,
        13,
        "M1.move",
        6,
        10,
        "M1.move",
        "const Symbols::M1::SOME_CONST: u64 = 42",
        None,
    );
    // function name (ret function)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        61,
        8,
        "M1.move",
        61,
        8,
        "M1.move",
        "fun Symbols::M1::ret(p1: bool, p2: u64): u64",
        None,
    );
    // returned value (ret function)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        63,
        19,
        "M1.move",
        6,
        10,
        "M1.move",
        "const Symbols::M1::SOME_CONST: u64 = 42",
        None,
    );
    // function name (abort_call function)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        68,
        8,
        "M1.move",
        68,
        8,
        "M1.move",
        "fun Symbols::M1::abort_call()",
        None,
    );
    // abort value (abort_call function)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        69,
        14,
        "M1.move",
        6,
        10,
        "M1.move",
        "const Symbols::M1::SOME_CONST: u64 = 42",
        None,
    );
    // dereference (deref function)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        75,
        9,
        "M1.move",
        74,
        12,
        "M1.move",
        "let r: &u64",
        None,
    );
    // unary operator (unary function)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        79,
        9,
        "M1.move",
        78,
        14,
        "M1.move",
        "p: bool",
        None,
    );
    // temp borrow (temp_borrow function)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        83,
        19,
        "M1.move",
        6,
        10,
        "M1.move",
        "const Symbols::M1::SOME_CONST: u64 = 42",
        None,
    );
    // chain access first element (chain_access function)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        94,
        8,
        "M1.move",
        93,
        12,
        "M1.move",
        "let outer: Symbols::M1::OuterStruct",
        Some((87, 11, "M1.move")),
    );
    // chain second element (chain_access function)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        94,
        14,
        "M1.move",
        88,
        8,
        "M1.move",
        "Symbols::M1::OuterStruct\nsome_struct: Symbols::M1::SomeStruct",
        Some((2, 11, "M1.move")),
    );
    // chain access third element (chain_access function)
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        94,
        26,
        "M1.move",
        3,
        8,
        "M1.move",
        "Symbols::M1::SomeStruct\nsome_field: u64",
        None,
    );
    // chain second element after the block (chain_access_block function)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        102,
        10,
        "M1.move",
        88,
        8,
        "M1.move",
        "Symbols::M1::OuterStruct\nsome_struct: Symbols::M1::SomeStruct",
        Some((2, 11, "M1.move")),
    );
    // chain access first element when borrowing (chain_access_borrow function)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        108,
        17,
        "M1.move",
        107,
        12,
        "M1.move",
        "let outer: Symbols::M1::OuterStruct",
        Some((87, 11, "M1.move")),
    );
    // chain second element when borrowing (chain_access_borrow function)
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        108,
        23,
        "M1.move",
        88,
        8,
        "M1.move",
        "Symbols::M1::OuterStruct\nsome_struct: Symbols::M1::SomeStruct",
        Some((2, 11, "M1.move")),
    );
    // chain access third element when borrowing (chain_access_borrow function)
    assert_use_def(
        mod_symbols,
        &symbols,
        3,
        108,
        35,
        "M1.move",
        3,
        8,
        "M1.move",
        "Symbols::M1::SomeStruct\nsome_field: u64",
        None,
    );
    // variable in cast (cast function)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        114,
        9,
        "M1.move",
        113,
        12,
        "M1.move",
        "let tmp: u128",
        None,
    );
    // constant in an annotation (annot function)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        118,
        19,
        "M1.move",
        6,
        10,
        "M1.move",
        "const Symbols::M1::SOME_CONST: u64 = 42",
        None,
    );
    // struct type param def (struct_param function)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        122,
        21,
        "M1.move",
        122,
        21,
        "M1.move",
        "p: Symbols::M2::SomeOtherStruct",
        Some((2, 11, "M2.move")),
    );
    // struct type param use (struct_param function)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        123,
        8,
        "M1.move",
        122,
        21,
        "M1.move",
        "p: Symbols::M2::SomeOtherStruct",
        Some((2, 11, "M2.move")),
    );
    // struct type local var def (struct_var function)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        127,
        12,
        "M1.move",
        127,
        12,
        "M1.move",
        "let tmp: Symbols::M2::SomeOtherStruct",
        Some((2, 11, "M2.move")),
    );
    // struct type local var use (struct_var function)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        129,
        12,
        "M1.move",
        127,
        12,
        "M1.move",
        "let tmp: Symbols::M2::SomeOtherStruct",
        Some((2, 11, "M2.move")),
    );

    let mut fpath = path.clone();
    fpath.push("sources/M3.move");
    let cpath = dunce::canonicalize(&fpath).unwrap();

    let mod_symbols = symbols.file_use_defs.get(&cpath).unwrap();

    // generic type in struct definition
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        2,
        23,
        "M3.move",
        2,
        23,
        "M3.move",
        "T",
        None,
    );
    // generic type in struct field definition
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        3,
        20,
        "M3.move",
        2,
        23,
        "M3.move",
        "T",
        None,
    );
    // generic type in generic type definition (type_param_arg function)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        6,
        23,
        "M3.move",
        6,
        23,
        "M3.move",
        "T",
        None,
    );
    // parameter (type_param_arg function)
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        6,
        39,
        "M3.move",
        6,
        39,
        "M3.move",
        "param: T",
        None,
    );
    // generic type in param type (type_param_arg function)
    assert_use_def(
        mod_symbols,
        &symbols,
        3,
        6,
        46,
        "M3.move",
        6,
        23,
        "M3.move",
        "T",
        None,
    );
    // generic type in return type (type_param_arg function)
    assert_use_def(
        mod_symbols,
        &symbols,
        4,
        6,
        50,
        "M3.move",
        6,
        23,
        "M3.move",
        "T",
        None,
    );
    // generic type in struct param type (struct_type_param_arg function)
    assert_use_def(
        mod_symbols,
        &symbols,
        4,
        10,
        52,
        "M3.move",
        10,
        30,
        "M3.move",
        "T",
        None,
    );
    // generic type in struct return type (struct_type_param_arg function)
    assert_use_def(
        mod_symbols,
        &symbols,
        6,
        10,
        69,
        "M3.move",
        10,
        30,
        "M3.move",
        "T",
        None,
    );
    // parameter (struct_type_param_arg function) of generic struct type
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        11,
        8,
        "M3.move",
        10,
        33,
        "M3.move",
        "param: Symbols::M3::ParamStruct<T>",
        Some((2, 11, "M3.move")),
    );
    // generic type in pack (pack_type_param function)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        15,
        20,
        "M3.move",
        14,
        24,
        "M3.move",
        "T",
        None,
    );
    // field type in struct field definition which itself is a struct
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        23,
        20,
        "M3.move",
        2,
        11,
        "M3.move",
        "struct Symbols::M3::ParamStruct<T> {\n\tsome_field: T\n}",
        Some((2, 11, "M3.move")),
    );
    // generic type in struct field definition which itself is a struct
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        23,
        32,
        "M3.move",
        22,
        30,
        "M3.move",
        "T",
        None,
    );

    let mut fpath = path.clone();
    fpath.push("sources/M4.move");
    let cpath = dunce::canonicalize(&fpath).unwrap();

    let mod_symbols = symbols.file_use_defs.get(&cpath).unwrap();

    // param name in RHS (if_cond function)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        4,
        18,
        "M4.move",
        2,
        16,
        "M4.move",
        "tmp: u64",
        None,
    );
    // param name in RHS (if_cond function)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        6,
        22,
        "M4.move",
        4,
        12,
        "M4.move",
        "let tmp: u64",
        None,
    );
    // var in if's true branch (if_cond function)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        7,
        12,
        "M4.move",
        4,
        12,
        "M4.move",
        "let tmp: u64",
        None,
    );
    // redefined var in if's false branch (if_cond function)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        10,
        12,
        "M4.move",
        9,
        16,
        "M4.move",
        "let tmp: u64",
        None,
    );
    // var name in while loop condition (while_loop function)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        20,
        15,
        "M4.move",
        18,
        12,
        "M4.move",
        "let tmp: u64",
        None,
    );
    // var name in while loop's inner block (while_loop function)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        23,
        26,
        "M4.move",
        18,
        12,
        "M4.move",
        "let tmp: u64",
        None,
    );
    // redefined var name in while loop's inner block (while_loop function)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        24,
        23,
        "M4.move",
        23,
        20,
        "M4.move",
        "let tmp: u64",
        None,
    );
    // var name in while loop's main block (while_loop function)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        26,
        12,
        "M4.move",
        18,
        12,
        "M4.move",
        "let tmp: u64",
        None,
    );
    // redefined var name in while loop's inner block (loop function)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        40,
        23,
        "M4.move",
        39,
        20,
        "M4.move",
        "let tmp: u64",
        None,
    );
    // var name in loop's main block (loop function)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        43,
        16,
        "M4.move",
        34,
        12,
        "M4.move",
        "let tmp: u64",
        None,
    );
    // const in a different module in the same file
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        55,
        10,
        "M4.move",
        55,
        10,
        "M4.move",
        "const Symbols::M5::SOME_CONST: u64 = 7",
        None,
    );
}

#[test]
/// Tests if symbolication information for constants has been constructed correctly.
fn const_test() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    path.push("tests/symbols");

    let ide_files_layer: VfsPath = MemoryFS::new().into();
    let (symbols_opt, _) = get_symbols(
        Arc::new(Mutex::new(BTreeMap::new())),
        ide_files_layer,
        path.as_path(),
        LintLevel::None,
    )
    .unwrap();
    let symbols = symbols_opt.unwrap();

    let mut fpath = path.clone();
    fpath.push("sources/M8.move");
    let cpath = dunce::canonicalize(&fpath).unwrap();

    let mod_symbols = symbols.file_use_defs.get(&cpath).unwrap();

    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        2,
        10,
        "M8.move",
        2,
        10,
        "M8.move",
        "const Symbols::M8::MY_BOOL: bool = false",
        None,
    );
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        4,
        10,
        "M8.move",
        4,
        10,
        "M8.move",
        "const Symbols::M8::PAREN: bool = true",
        None,
    );
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        6,
        10,
        "M8.move",
        6,
        10,
        "M8.move",
        "const Symbols::M8::BLOCK: bool = true",
        None,
    );
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        8,
        10,
        "M8.move",
        8,
        10,
        "M8.move",
        "const Symbols::M8::MY_ADDRESS: address = @0x70DD",
        None,
    );
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        10,
        10,
        "M8.move",
        10,
        10,
        "M8.move",
        "const Symbols::M8::BYTES: vector<u8> = [104, 101, 108, 108, 111, 32, 119, 111, 114, 108, 100]",
        None,
    );
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        12,
        10,
        "M8.move",
        12,
        10,
        "M8.move",
        "const Symbols::M8::HEX_BYTES: vector<u8> = [222, 173, 190, 239]",
        None,
    );
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        14,
        10,
        "M8.move",
        14,
        10,
        "M8.move",
        "const Symbols::M8::NUMS: vector<u16> = [1, 2]",
        None,
    );
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        16,
        10,
        "M8.move",
        16,
        10,
        "M8.move",
        "const Symbols::M8::RULE: bool = true && false",
        None,
    );
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        18,
        10,
        "M8.move",
        18,
        10,
        "M8.move",
        "const Symbols::M8::CAP: u64 = 10 * 100 + 1",
        None,
    );
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        20,
        10,
        "M8.move",
        20,
        10,
        "M8.move",
        "const Symbols::M8::SHIFTY: u8 = 1 << 1",
        None,
    );
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        22,
        10,
        "M8.move",
        22,
        10,
        "M8.move",
        "const Symbols::M8::HALF_MAX: u128 = 340282366920938463463374607431768211455 / 2",
        None,
    );
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        24,
        10,
        "M8.move",
        24,
        10,
        "M8.move",
        "const Symbols::M8::REM: u256 = 57896044618658097711785492504343953926634992332820282019728792003956564819968 % 654321",
        None,
    );
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        26,
        10,
        "M8.move",
        26,
        10,
        "M8.move",
        "const Symbols::M8::USE_CONST: bool = Symbols::M8::EQUAL == false",
        None,
    );
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        26,
        28,
        "M8.move",
        28,
        10,
        "M8.move",
        "const Symbols::M8::EQUAL: bool = 1 == 1",
        None,
    );
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        28,
        10,
        "M8.move",
        28,
        10,
        "M8.move",
        "const Symbols::M8::EQUAL: bool = 1 == 1",
        None,
    );
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        30,
        10,
        "M8.move",
        30,
        10,
        "M8.move",
        "const Symbols::M8::ANOTHER_USE_CONST: bool = Symbols::M8::EQUAL == false",
        None,
    );
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        30,
        49,
        "M8.move",
        28,
        10,
        "M8.move",
        "const Symbols::M8::EQUAL: bool = 1 == 1",
        None,
    );
}

#[test]
/// Tests if symbolication information for imports (use statements) has been constructed correctly.
fn imports_test() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    path.push("tests/symbols");

    let ide_files_layer: VfsPath = MemoryFS::new().into();
    let (symbols_opt, _) = get_symbols(
        Arc::new(Mutex::new(BTreeMap::new())),
        ide_files_layer,
        path.as_path(),
        LintLevel::None,
    )
    .unwrap();
    let symbols = symbols_opt.unwrap();

    let mut fpath = path.clone();
    fpath.push("sources/M9.move");
    let cpath = dunce::canonicalize(&fpath).unwrap();

    let mod_symbols = symbols.file_use_defs.get(&cpath).unwrap();

    // simple doc-commented mod use from different mod (same file)
    assert_use_def_with_doc_string(
        mod_symbols,
        &symbols,
        0,
        1,
        16,
        "M9.move",
        5,
        16,
        "M9.move",
        "module Symbols::M9",
        None,
        Some("A module doc comment\n"),
    );
    // simple mod use from different mod (different file)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        7,
        17,
        "M9.move",
        0,
        16,
        "M1.move",
        "module Symbols::M1",
        None,
    );
    // aliased mod use (actual mod name)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        8,
        17,
        "M9.move",
        0,
        16,
        "M1.move",
        "module Symbols::M1",
        None,
    );
    // aliased mod use (alias name)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        8,
        23,
        "M9.move",
        0,
        16,
        "M1.move",
        "module Symbols::M1",
        None,
    );
    // aliased mod use from mod list - first element (actual name)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        9,
        18,
        "M9.move",
        0,
        16,
        "M1.move",
        "module Symbols::M1",
        None,
    );
    // aliased mod use from mod list - first element (alias name)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        9,
        24,
        "M9.move",
        0,
        16,
        "M1.move",
        "module Symbols::M1",
        None,
    );
    // aliased mod use from mod list - second element (actual name)
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        9,
        30,
        "M9.move",
        0,
        16,
        "M2.move",
        "module Symbols::M2",
        None,
    );
    // aliased mod use from mod list - second element (alias name)
    assert_use_def(
        mod_symbols,
        &symbols,
        3,
        9,
        36,
        "M9.move",
        0,
        16,
        "M2.move",
        "module Symbols::M2",
        None,
    );
    // aliased struct import (actual name)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        10,
        22,
        "M9.move",
        2,
        11,
        "M2.move",
        "struct Symbols::M2::SomeOtherStruct has drop {\n	some_field: u64\n}",
        Some((2, 11, "M2.move")),
    );
    // aliased mod use (alias name)
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        10,
        41,
        "M9.move",
        2,
        11,
        "M2.move",
        "struct Symbols::M2::SomeOtherStruct has drop {\n	some_field: u64\n}",
        Some((2, 11, "M2.move")),
    );
    // locally aliased mod use (actual mod name)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        32,
        21,
        "M9.move",
        0,
        16,
        "M1.move",
        "module Symbols::M1",
        None,
    );
    // locally aliased mod use (alias name)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        32,
        27,
        "M9.move",
        0,
        16,
        "M1.move",
        "module Symbols::M1",
        None,
    );
    // aliased struct use
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        37,
        27,
        "M9.move",
        2,
        11,
        "M2.move",
        "struct Symbols::M2::SomeOtherStruct has drop {\n	some_field: u64\n}",
        Some((2, 11, "M2.move")),
    );
}

#[test]
/// Tests if symbolication information for module accesses has been constructed correctly.
fn module_access_test() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    path.push("tests/symbols");

    let ide_files_layer: VfsPath = MemoryFS::new().into();
    let (symbols_opt, _) = get_symbols(
        Arc::new(Mutex::new(BTreeMap::new())),
        ide_files_layer,
        path.as_path(),
        LintLevel::None,
    )
    .unwrap();
    let symbols = symbols_opt.unwrap();

    let mut fpath = path.clone();
    fpath.push("sources/M9.move");
    let cpath = dunce::canonicalize(&fpath).unwrap();

    let mod_symbols = symbols.file_use_defs.get(&cpath).unwrap();

    // fully qualified module access in return type
    assert_use_def_with_doc_string(
        mod_symbols,
        &symbols,
        1,
        18,
        32,
        "M9.move",
        5,
        16,
        "M9.move",
        "module Symbols::M9",
        None,
        Some("A module doc comment\n"),
    );
    // fully qualified module access in struct type (pack)
    assert_use_def_with_doc_string(
        mod_symbols,
        &symbols,
        0,
        19,
        17,
        "M9.move",
        5,
        16,
        "M9.move",
        "module Symbols::M9",
        None,
        Some("A module doc comment\n"),
    );
    // fully qualified module access in constant access
    assert_use_def_with_doc_string(
        mod_symbols,
        &symbols,
        3,
        19,
        55,
        "M9.move",
        5,
        16,
        "M9.move",
        "module Symbols::M9",
        None,
        Some("A module doc comment\n"),
    );
    // fully qualified module access in parameter type
    assert_use_def_with_doc_string(
        mod_symbols,
        &symbols,
        2,
        22,
        34,
        "M9.move",
        5,
        16,
        "M9.move",
        "module Symbols::M9",
        None,
        Some("A module doc comment\n"),
    );
    // fully qualified module access in struct type (unpack)
    assert_use_def_with_doc_string(
        mod_symbols,
        &symbols,
        0,
        23,
        21,
        "M9.move",
        5,
        16,
        "M9.move",
        "module Symbols::M9",
        None,
        Some("A module doc comment\n"),
    );
    // imported module access in parameter type
    assert_use_def_with_doc_string(
        mod_symbols,
        &symbols,
        2,
        27,
        34,
        "M9.move",
        0,
        16,
        "M1.move",
        "module Symbols::M1",
        None,
        None,
    );
    // imported aliased module access in return type
    assert_use_def_with_doc_string(
        mod_symbols,
        &symbols,
        4,
        27,
        51,
        "M9.move",
        0,
        16,
        "M1.move",
        "module Symbols::M1",
        None,
        None,
    );
    // imported locally aliased module access in local var type
    assert_use_def_with_doc_string(
        mod_symbols,
        &symbols,
        1,
        33,
        17,
        "M9.move",
        0,
        16,
        "M1.move",
        "module Symbols::M1",
        None,
        None,
    );
    // fully qualified module access in function call
    assert_use_def_with_doc_string(
        mod_symbols,
        &symbols,
        3,
        33,
        57,
        "M9.move",
        5,
        16,
        "M9.move",
        "module Symbols::M9",
        None,
        Some("A module doc comment\n"),
    );
}

#[test]
/// Tests if in presence of parsing errors for one module (M1), symbolication information will still
/// be correctly constructed for another independent module (M2).
fn parse_error_test() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    path.push("tests/parse-error");

    let ide_files_layer: VfsPath = MemoryFS::new().into();
    let (symbols_opt, _) = get_symbols(
        Arc::new(Mutex::new(BTreeMap::new())),
        ide_files_layer,
        path.as_path(),
        LintLevel::None,
    )
    .unwrap();
    let symbols = symbols_opt.unwrap();

    let mut fpath = path.clone();

    fpath.push("sources/M1.move");
    let cpath = dunce::canonicalize(&fpath).unwrap();

    let mod_symbols = symbols.file_use_defs.get(&cpath).unwrap();
    // const in a file containing a parse error
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        8,
        10,
        "M1.move",
        8,
        10,
        "M1.move",
        "const ParseError::M1::c: u64 = 7",
        None,
    );
    // const in a file containing a parse error (in the second module, after parsing error in the
    // previous module)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        14,
        10,
        "M1.move",
        14,
        10,
        "M1.move",
        "const ParseError::M3::c: u64 = 7",
        None,
    );
    // const in a file containing a parse error (in the second module, with module annotation, after
    // parsing error in the previous module)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        21,
        10,
        "M1.move",
        21,
        10,
        "M1.move",
        "const ParseError::M4::c: u64 = 7",
        None,
    );

    let mut fpath = path.clone();
    fpath.push("sources/M2.move");
    let cpath = dunce::canonicalize(&fpath).unwrap();

    let mod_symbols = symbols.file_use_defs.get(&cpath).unwrap();

    // struct def in the same file
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        2,
        11,
        "M2.move",
        2,
        11,
        "M2.move",
        "struct ParseError::M2::SomeStruct {\n\tsome_field: u64\n}",
        Some((2, 11, "M2.move")),
    );
}

#[test]
/// Tests if in presence of parsing errors for one module (M1), partial symbolication information
/// will still be correctly constructed for another dependent module (M2).
fn parse_error_with_deps_test() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    path.push("tests/parse-error-dep");

    let ide_files_layer: VfsPath = MemoryFS::new().into();
    let (symbols_opt, _) = get_symbols(
        Arc::new(Mutex::new(BTreeMap::new())),
        ide_files_layer,
        path.as_path(),
        LintLevel::None,
    )
    .unwrap();
    let symbols = symbols_opt.unwrap();

    let mut fpath = path.clone();
    fpath.push("sources/M2.move");
    let cpath = dunce::canonicalize(&fpath).unwrap();

    let mod_symbols = symbols.file_use_defs.get(&cpath).unwrap();

    // function def in the same file
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        4,
        15,
        "M2.move",
        4,
        15,
        "M2.move",
        "public fun ParseErrorDep::M2::fun_call(): u64",
        None,
    );

    // arg def of unknown type (unresolved from a non-parseable module)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        8,
        29,
        "M2.move",
        8,
        29,
        "M2.move",
        "s: ParseErrorDep::M1::SomeStruct",
        Some((2, 11, "M1.move")),
    );
}

#[test]
/// Tests if in presence of pre-typing (e.g. in naming) errors for one module (M1), symbolication
/// information will still be correctly constructed for another independent module (M2).
fn pretype_error_test() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    path.push("tests/pre-type-error");

    let ide_files_layer: VfsPath = MemoryFS::new().into();
    let (symbols_opt, _) = get_symbols(
        Arc::new(Mutex::new(BTreeMap::new())),
        ide_files_layer,
        path.as_path(),
        LintLevel::None,
    )
    .unwrap();
    let symbols = symbols_opt.unwrap();

    let mut fpath = path.clone();
    fpath.push("sources/M2.move");
    let cpath = dunce::canonicalize(&fpath).unwrap();

    let mod_symbols = symbols.file_use_defs.get(&cpath).unwrap();

    // struct def in the same file
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        2,
        11,
        "M2.move",
        2,
        11,
        "M2.move",
        "struct PreTypeError::M2::SomeStruct {\n\tsome_field: u64\n}",
        Some((2, 11, "M2.move")),
    );
}

#[test]
/// Tests if in presence of pre-typing (e.g. in naming) errors for one module (M1), partial
/// symbolication information will still be correctly constructed for another dependent module (M2)
/// or even for a module with the error.
fn pretype_error_with_deps_test() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    path.push("tests/pre-type-error-dep");

    let ide_files_layer: VfsPath = MemoryFS::new().into();
    let (symbols_opt, _) = get_symbols(
        Arc::new(Mutex::new(BTreeMap::new())),
        ide_files_layer,
        path.as_path(),
        LintLevel::None,
    )
    .unwrap();
    let symbols = symbols_opt.unwrap();

    let mut fpath = path.clone();
    fpath.push("sources/M1.move");
    let cpath = dunce::canonicalize(&fpath).unwrap();

    let mod_symbols = symbols.file_use_defs.get(&cpath).unwrap();

    // struct def in the file containing an error
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        2,
        11,
        "M1.move",
        2,
        11,
        "M1.move",
        "struct PreTypeErrorDep::M1::SomeStruct {\n\tsome_field: u64\n}",
        Some((2, 11, "M1.move")),
    );

    // fun def in the file containing an error inside this fun body
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        12,
        8,
        "M1.move",
        12,
        8,
        "M1.move",
        "fun PreTypeErrorDep::M1::wrong(): address",
        None,
    );

    let mut fpath = path.clone();
    fpath.push("sources/M2.move");
    let cpath = dunce::canonicalize(&fpath).unwrap();

    let mod_symbols = symbols.file_use_defs.get(&cpath).unwrap();

    // function def in the same file
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        4,
        15,
        "M2.move",
        4,
        15,
        "M2.move",
        "public fun PreTypeErrorDep::M2::fun_call(): u64",
        None,
    );

    // arg def of type defined in a module containing an error
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        8,
        29,
        "M2.move",
        8,
        29,
        "M2.move",
        "s: PreTypeErrorDep::M1::SomeStruct",
        Some((2, 11, "M1.move")),
    );
    // function call (to a function defined in a module containing errors)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        5,
        29,
        "M2.move",
        6,
        15,
        "M1.move",
        "public fun PreTypeErrorDep::M1::foo(): u64",
        None,
    );
}

#[test]
/// Tests symbolication of constructs related to dot call syntax.
fn dot_call_test() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    path.push("tests/move-2024");

    let ide_files_layer: VfsPath = MemoryFS::new().into();
    let (symbols_opt, _) = get_symbols(
        Arc::new(Mutex::new(BTreeMap::new())),
        ide_files_layer,
        path.as_path(),
        LintLevel::None,
    )
    .unwrap();
    let symbols = symbols_opt.unwrap();

    let mut fpath = path.clone();
    fpath.push("sources/dot_call.move");
    let cpath = dunce::canonicalize(&fpath).unwrap();

    let mod_symbols = symbols.file_use_defs.get(&cpath).unwrap();

    // the Self module name in public module use fun decl (for target fun)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        2,
        19,
        "dot_call.move",
        0,
        17,
        "dot_call.move",
        "module Move2024::M1",
        None,
    );
    // target fun in public module use fun decl
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        2,
        25,
        "dot_call.move",
        13,
        15,
        "dot_call.move",
        "public fun Move2024::M1::foo(s: &Move2024::M1::SomeStruct): u64",
        None,
    );
    // type in public module use fun decl
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        2,
        32,
        "dot_call.move",
        5,
        18,
        "dot_call.move",
        "public struct Move2024::M1::SomeStruct has drop {\n\tsome_field: u64\n}",
        Some((5, 18, "dot_call.move")),
    );
    // method in public module use fun decl
    assert_use_def(
        mod_symbols,
        &symbols,
        3,
        2,
        43,
        "dot_call.move",
        13,
        15,
        "dot_call.move",
        "public fun Move2024::M1::foo(s: &Move2024::M1::SomeStruct): u64",
        None,
    );
    // module name in public module use fun decl (for target fun)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        3,
        29,
        "dot_call.move",
        0,
        17,
        "dot_call.move",
        "module Move2024::M1",
        None,
    );
    // target fun in public module use fun decl
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        3,
        33,
        "dot_call.move",
        13,
        15,
        "dot_call.move",
        "public fun Move2024::M1::foo(s: &Move2024::M1::SomeStruct): u64",
        None,
    );
    // type in public module use fun decl
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        3,
        40,
        "dot_call.move",
        5,
        18,
        "dot_call.move",
        "public struct Move2024::M1::SomeStruct has drop {\n\tsome_field: u64\n}",
        Some((5, 18, "dot_call.move")),
    );
    // method in public module use fun decl
    assert_use_def(
        mod_symbols,
        &symbols,
        3,
        3,
        51,
        "dot_call.move",
        13,
        15,
        "dot_call.move",
        "public fun Move2024::M1::foo(s: &Move2024::M1::SomeStruct): u64",
        None,
    );

    // aliased module name in module use fun decl (for target fun)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        26,
        12,
        "dot_call.move",
        0,
        17,
        "dot_call.move",
        "module Move2024::M1",
        None,
    );
    // target fun in module use fun decl
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        26,
        22,
        "dot_call.move",
        17,
        15,
        "dot_call.move",
        "public fun Move2024::M1::bar(s: &Move2024::M1::SomeStruct, v: u64): u64",
        None,
    );
    // module name in module use fun decl (for type)
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        26,
        39,
        "dot_call.move",
        0,
        17,
        "dot_call.move",
        "module Move2024::M1",
        None,
    );
    // type in module use fun decl
    assert_use_def(
        mod_symbols,
        &symbols,
        3,
        26,
        43,
        "dot_call.move",
        5,
        18,
        "dot_call.move",
        "public struct Move2024::M1::SomeStruct has drop {\n\tsome_field: u64\n}",
        Some((5, 18, "dot_call.move")),
    );
    // method in module use fun decl
    assert_use_def(
        mod_symbols,
        &symbols,
        4,
        26,
        54,
        "dot_call.move",
        17,
        15,
        "dot_call.move",
        "public fun Move2024::M1::bar(s: &Move2024::M1::SomeStruct, v: u64): u64",
        None,
    );
    // module name in block use fun decl (for target fun)
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        29,
        16,
        "dot_call.move",
        0,
        17,
        "dot_call.move",
        "module Move2024::M1",
        None,
    );
    // target fun in block use fun decl
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        29,
        20,
        "dot_call.move",
        17,
        15,
        "dot_call.move",
        "public fun Move2024::M1::bar(s: &Move2024::M1::SomeStruct, v: u64): u64",
        None,
    );
    // aliased type in block use fun decl
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        29,
        27,
        "dot_call.move",
        5,
        18,
        "dot_call.move",
        "public struct Move2024::M1::SomeStruct has drop {\n\tsome_field: u64\n}",
        Some((5, 18, "dot_call.move")),
    );
    // method in block use fun decl
    assert_use_def(
        mod_symbols,
        &symbols,
        3,
        29,
        43,
        "dot_call.move",
        17,
        15,
        "dot_call.move",
        "public fun Move2024::M1::bar(s: &Move2024::M1::SomeStruct, v: u64): u64",
        None,
    );
    // receiver in a dot-call
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        33,
        16,
        "dot_call.move",
        31,
        12,
        "dot_call.move",
        "let some_struct: Move2024::M1::SomeStruct",
        Some((5, 18, "dot_call.move")),
    );
    // dot-call (one arg)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        33,
        28,
        "dot_call.move",
        13,
        15,
        "dot_call.move",
        "public fun Move2024::M1::foo(s: &Move2024::M1::SomeStruct): u64",
        None,
    );
    // receiver in a dot-call
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        34,
        16,
        "dot_call.move",
        31,
        12,
        "dot_call.move",
        "let some_struct: Move2024::M1::SomeStruct",
        Some((5, 18, "dot_call.move")),
    );
    // dot-call (one arg)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        34,
        28,
        "dot_call.move",
        17,
        15,
        "dot_call.move",
        "public fun Move2024::M1::bar(s: &Move2024::M1::SomeStruct, v: u64): u64",
        None,
    );
    // first arg in a dot-call
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        34,
        31,
        "dot_call.move",
        32,
        12,
        "dot_call.move",
        "let val: u64",
        None,
    );
}

#[test]
/// Checks if module identifiers used during symbolication process at both parsing and typing are
/// the same. They are used as a key to a map and if they look differently, it may lead to a crash
/// due to keys used for insertion/ retrieval being different.
fn mod_ident_uniform_test() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    path.push("tests/mod-ident-uniform");

    let ide_files_layer: VfsPath = MemoryFS::new().into();
    let (symbols_opt, _) = get_symbols(
        Arc::new(Mutex::new(BTreeMap::new())),
        ide_files_layer,
        path.as_path(),
        LintLevel::None,
    )
    .unwrap();
    let symbols = symbols_opt.unwrap();

    let mut fpath = path.clone();
    fpath.push("sources/M1.move");
    let cpath = dunce::canonicalize(&fpath).unwrap();

    symbols.file_use_defs.get(&cpath).unwrap();
}

#[test]
/// Checks if symbolication for positional struct fields work correctly and recognizes the
/// visibility modifier.
fn move2024_struct_test() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    path.push("tests/move-2024");

    let ide_files_layer: VfsPath = MemoryFS::new().into();
    let (symbols_opt, _) = get_symbols(
        Arc::new(Mutex::new(BTreeMap::new())),
        ide_files_layer,
        path.as_path(),
        LintLevel::None,
    )
    .unwrap();
    let symbols = symbols_opt.unwrap();

    let mut fpath = path.clone();
    fpath.push("sources/structs.move");
    let cpath = dunce::canonicalize(&fpath).unwrap();

    let mod_symbols = symbols.file_use_defs.get(&cpath).unwrap();

    // struct def with no fields
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        2,
        18,
        "structs.move",
        2,
        18,
        "structs.move",
        "public struct Move2024::structs::SomeStruct has copy, drop {}",
        Some((2, 18, "structs.move")),
    );
    // struct def with positional fields
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        4,
        18,
        "structs.move",
        4,
        18,
        "structs.move",
        "public struct Move2024::structs::Positional has copy, drop {\n\t0: u64,\n\t1: Move2024::structs::SomeStruct\n}",
        Some((4, 18, "structs.move")),
    );
    // positional field type (in def)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        4,
        34,
        "structs.move",
        2,
        18,
        "structs.move",
        "public struct Move2024::structs::SomeStruct has copy, drop {}",
        Some((2, 18, "structs.move")),
    );
    // fun param of a positional struct type
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        6,
        19,
        "structs.move",
        6,
        19,
        "structs.move",
        "positional: Move2024::structs::Positional",
        Some((4, 18, "structs.move")),
    );
    // fun param type of a positional struct type
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        6,
        31,
        "structs.move",
        4,
        18,
        "structs.move",
        "public struct Move2024::structs::Positional has copy, drop {\n\t0: u64,\n\t1: Move2024::structs::SomeStruct\n}",
        Some((4, 18, "structs.move")),
    );
    // first positional field access (u64 type)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        7,
        20,
        "structs.move",
        4,
        29,
        "structs.move",
        "Move2024::structs::Positional\n0: u64",
        None,
    );
    // first positional field access (struct type)
    assert_use_def(
        mod_symbols,
        &symbols,
        3,
        7,
        34,
        "structs.move",
        4,
        34,
        "structs.move",
        "Move2024::structs::Positional\n1: Move2024::structs::SomeStruct",
        Some((2, 18, "structs.move")),
    );
}

#[test]
/// Tests symbolication of implicit structs and modules.
fn implicit_uses_test() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    path.push("tests/move-2024");

    let ide_files_layer: VfsPath = MemoryFS::new().into();
    let (symbols_opt, _) = get_symbols(
        Arc::new(Mutex::new(BTreeMap::new())),
        ide_files_layer,
        path.as_path(),
        LintLevel::None,
    )
    .unwrap();
    let symbols = symbols_opt.unwrap();

    let mut fpath = path.clone();
    fpath.push("sources/implicit_uses.move");
    let cpath = dunce::canonicalize(&fpath).unwrap();

    let mod_symbols = symbols.file_use_defs.get(&cpath).unwrap();

    // implicit struct in field def
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        3,
        13,
        "implicit_uses.move",
        6,
        11,
        "option.move",
        "public struct std::option::Option<Element> has copy, drop, store {\n\tvec: vector<Element>\n}",
        Some((6, 11, "option.move")),
    );
    // implicit module name in function call
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        7,
        26,
        "implicit_uses.move",
        1,
        12,
        "option.move",
        "module std::option",
        None,
    );
}

#[test]
/// Tests mutability annotation added in Move 2024.
fn let_mut_test() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    path.push("tests/move-2024");

    let ide_files_layer: VfsPath = MemoryFS::new().into();
    let (symbols_opt, _) = get_symbols(
        Arc::new(Mutex::new(BTreeMap::new())),
        ide_files_layer,
        path.as_path(),
        LintLevel::None,
    )
    .unwrap();
    let symbols = symbols_opt.unwrap();

    let mut fpath = path.clone();
    fpath.push("sources/let_mut.move");
    let cpath = dunce::canonicalize(&fpath).unwrap();

    let mod_symbols = symbols.file_use_defs.get(&cpath).unwrap();

    // mut param def
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        2,
        23,
        "let_mut.move",
        2,
        23,
        "let_mut.move",
        "mut p: u64",
        None,
    );
    // mut param use
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        3,
        8,
        "let_mut.move",
        2,
        23,
        "let_mut.move",
        "mut p: u64",
        None,
    );
    // mut var def
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        4,
        16,
        "let_mut.move",
        4,
        16,
        "let_mut.move",
        "let mut v: u64",
        None,
    );
    // mut var use
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        5,
        8,
        "let_mut.move",
        4,
        16,
        "let_mut.move",
        "let mut v: u64",
        None,
    );
}

#[test]
/// Tests symbolication of partially defined function
fn partial_function_test() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    path.push("tests/partial-function");

    let ide_files_layer: VfsPath = MemoryFS::new().into();
    let (symbols_opt, _) = get_symbols(
        Arc::new(Mutex::new(BTreeMap::new())),
        ide_files_layer,
        path.as_path(),
        LintLevel::None,
    )
    .unwrap();
    let symbols = symbols_opt.unwrap();

    let mut fpath = path.clone();
    fpath.push("sources/M1.move");
    let cpath = dunce::canonicalize(&fpath).unwrap();

    symbols.file_use_defs.get(&cpath).unwrap();

    let mod_symbols = symbols.file_use_defs.get(&cpath).unwrap();

    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        2,
        8,
        "M1.move",
        2,
        8,
        "M1.move",
        "fun PartialFunction::M1::just_name()",
        None,
    );
}

#[test]
/// Tests if partial dot chains are symbolicated correctly.
fn partial_dot_test() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    path.push("tests/partial-dot");

    let ide_files_layer: VfsPath = MemoryFS::new().into();
    let (symbols_opt, _) = get_symbols(
        Arc::new(Mutex::new(BTreeMap::new())),
        ide_files_layer,
        path.as_path(),
        LintLevel::None,
    )
    .unwrap();
    let symbols = symbols_opt.unwrap();

    let mut fpath = path.clone();
    fpath.push("sources/M1.move");
    let cpath = dunce::canonicalize(&fpath).unwrap();

    let mod_symbols = symbols.file_use_defs.get(&cpath).unwrap();

    // struct-typed first part of incomplete dot chain `s.;`
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        10,
        20,
        "M1.move",
        9,
        12,
        "M1.move",
        "s: PartialDot::M1::AnotherStruct",
        Some((5, 18, "M1.move")),
    );
    // struct-typed first part of incomplete dot chain `s.another_field.;`
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        11,
        20,
        "M1.move",
        9,
        12,
        "M1.move",
        "s: PartialDot::M1::AnotherStruct",
        Some((5, 18, "M1.move")),
    );
    // struct-typed second part of incomplete dot chain `s.another_field.;`
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        11,
        22,
        "M1.move",
        6,
        8,
        "M1.move",
        "PartialDot::M1::AnotherStruct\nanother_field: PartialDot::M1::SomeStruct",
        Some((1, 18, "M1.move")),
    );
    // struct-typed second part of incomplete dot chain `s.another_field.` (no `;` but followed by
    // `let` on the next line)
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        12,
        22,
        "M1.move",
        6,
        8,
        "M1.move",
        "PartialDot::M1::AnotherStruct\nanother_field: PartialDot::M1::SomeStruct",
        Some((1, 18, "M1.move")),
    );
    // struct-typed second part of incomplete dot chain `s.another_field.` (followed by a list of
    // parameters and a semi-colon: `s.another_field.(7, 42);`)
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        14,
        22,
        "M1.move",
        6,
        8,
        "M1.move",
        "PartialDot::M1::AnotherStruct\nanother_field: PartialDot::M1::SomeStruct",
        Some((1, 18, "M1.move")),
    );
    // struct-typed first part of incomplete dot chain `s.` (no `;` but followed by `}` on the next
    // line)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        15,
        20,
        "M1.move",
        9,
        12,
        "M1.move",
        "s: PartialDot::M1::AnotherStruct",
        Some((5, 18, "M1.move")),
    );
}

#[test]
/// Checks if a renamed dependent package is handled correctly, which could otherwise lead to a
/// crash due to package identity being seen differently when processing parsed AST and when
/// processing typed AST.
fn pkg_renaming_test() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    path.push("tests/pkg-naming-error");

    let ide_files_layer: VfsPath = MemoryFS::new().into();
    let (symbols_opt, _) = get_symbols(
        Arc::new(Mutex::new(BTreeMap::new())),
        ide_files_layer,
        path.as_path(),
        LintLevel::None,
    )
    .unwrap();
    let symbols = symbols_opt.unwrap();

    let mut fpath = path.clone();
    fpath.push("sources/M1.move");
    let cpath = dunce::canonicalize(&fpath).unwrap();

    symbols.file_use_defs.get(&cpath).unwrap();

    let mut fpath = path.clone();
    fpath.push("sources/M2.move");
    let cpath = dunce::canonicalize(&fpath).unwrap();

    symbols.file_use_defs.get(&cpath).unwrap();
}

#[test]
/// Tests if function types (`entry`, `macro`) are displayed correctly
fn function_types_test() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    path.push("tests/macros");

    let ide_files_layer: VfsPath = MemoryFS::new().into();
    let (symbols_opt, _) = get_symbols(
        Arc::new(Mutex::new(BTreeMap::new())),
        ide_files_layer,
        path.as_path(),
        LintLevel::None,
    )
    .unwrap();
    let symbols = symbols_opt.unwrap();

    let mut fpath = path.clone();
    fpath.push("sources/fun_type.move");
    let cpath = dunce::canonicalize(&fpath).unwrap();

    symbols.file_use_defs.get(&cpath).unwrap();

    let mod_symbols = symbols.file_use_defs.get(&cpath).unwrap();

    // entry function definition
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        2,
        14,
        "fun_type.move",
        2,
        14,
        "fun_type.move",
        "entry fun Macros::fun_type::entry_fun()",
        None,
    );
    // macro function definition
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        5,
        14,
        "fun_type.move",
        5,
        14,
        "fun_type.move",
        "macro fun Macros::fun_type::macro_fun()",
        None,
    );

    // entry function call
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        9,
        8,
        "fun_type.move",
        2,
        14,
        "fun_type.move",
        "entry fun Macros::fun_type::entry_fun()",
        None,
    );
    // macro function call
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        10,
        8,
        "fun_type.move",
        5,
        14,
        "fun_type.move",
        "macro fun Macros::fun_type::macro_fun()",
        None,
    );
}

#[test]
/// Tests macro definitions and invocations
fn macros_test() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    path.push("tests/macros");

    let ide_files_layer: VfsPath = MemoryFS::new().into();
    let (symbols_opt, _) = get_symbols(
        Arc::new(Mutex::new(BTreeMap::new())),
        ide_files_layer,
        path.as_path(),
        LintLevel::None,
    )
    .unwrap();
    let symbols = symbols_opt.unwrap();

    let mut fpath = path.clone();
    fpath.push("sources/macros.move");
    let cpath = dunce::canonicalize(&fpath).unwrap();

    symbols.file_use_defs.get(&cpath).unwrap();

    let mod_symbols = symbols.file_use_defs.get(&cpath).unwrap();

    // macro definitions - the signature should be symbolicated including lambda types etc.

    // macro name
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        6,
        14,
        "macros.move",
        6,
        14,
        "macros.move",
        "macro fun Macros::macros::foo($i: u64, $body: |u64| -> u64): u64",
        None,
    );
    // first non-lambda param (primitive type)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        6,
        18,
        "macros.move",
        6,
        18,
        "macros.move",
        "$i: u64",
        None,
    );
    // second lambda param (using primitive types)
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        6,
        27,
        "macros.move",
        6,
        27,
        "macros.move",
        "$body: |u64| -> u64",
        None,
    );

    // macro name
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        14,
        14,
        "macros.move",
        14,
        14,
        "macros.move",
        "macro fun Macros::macros::bar($i: Macros::macros::SomeStruct, $body: |Macros::macros::SomeStruct| -> Macros::macros::SomeStruct): Macros::macros::SomeStruct",
        None,
    );
    // first non-lambda param (struct type)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        14,
        18,
        "macros.move",
        14,
        18,
        "macros.move",
        "$i: Macros::macros::SomeStruct",
        Some((2, 18, "macros.move")),
    );
    // first non-lambda param type (struct type)
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        14,
        22,
        "macros.move",
        2,
        18,
        "macros.move",
        "public struct Macros::macros::SomeStruct has drop {\n\tsome_field: u64\n}",
        Some((2, 18, "macros.move")),
    );
    // second lambda param (using struct types)
    assert_use_def(
        mod_symbols,
        &symbols,
        3,
        14,
        34,
        "macros.move",
        14,
        34,
        "macros.move",
        "$body: |Macros::macros::SomeStruct| -> Macros::macros::SomeStruct",
        None,
    );
    // lambda param type (struct type)
    assert_use_def(
        mod_symbols,
        &symbols,
        4,
        14,
        42,
        "macros.move",
        2,
        18,
        "macros.move",
        "public struct Macros::macros::SomeStruct has drop {\n\tsome_field: u64\n}",
        Some((2, 18, "macros.move")),
    );
    // lambda param type (struct type)
    assert_use_def(
        mod_symbols,
        &symbols,
        5,
        14,
        57,
        "macros.move",
        2,
        18,
        "macros.move",
        "public struct Macros::macros::SomeStruct has drop {\n\tsome_field: u64\n}",
        Some((2, 18, "macros.move")),
    );
    // macro ret type (struct type)
    assert_use_def(
        mod_symbols,
        &symbols,
        6,
        14,
        70,
        "macros.move",
        2,
        18,
        "macros.move",
        "public struct Macros::macros::SomeStruct has drop {\n\tsome_field: u64\n}",
        Some((2, 18, "macros.move")),
    );

    // macro name
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        18,
        14,
        "macros.move",
        18,
        14,
        "macros.move",
        "macro fun Macros::macros::for_each<$T>($v: &vector<$T>, $body: |&$T| -> ())",
        None,
    );
    // macro's generic type
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        18,
        23,
        "macros.move",
        18,
        23,
        "macros.move",
        "$T",
        None,
    );
    // first non-lambda param (parameterized vec type)
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        18,
        27,
        "macros.move",
        18,
        27,
        "macros.move",
        "let $v: &vector<u64>",
        None,
    );
    // first non-lambda param type's generic type
    assert_use_def(
        mod_symbols,
        &symbols,
        3,
        18,
        39,
        "macros.move",
        18,
        23,
        "macros.move",
        "$T",
        None,
    );
    // second lambda param (using generic types)
    assert_use_def(
        mod_symbols,
        &symbols,
        4,
        18,
        44,
        "macros.move",
        18,
        44,
        "macros.move",
        "$body: |&$T| -> ()",
        None,
    );
    // lambda param type (struct type)
    assert_use_def(
        mod_symbols,
        &symbols,
        5,
        18,
        53,
        "macros.move",
        18,
        23,
        "macros.move",
        "$T",
        None,
    );

    // macro uses

    // module in macro call
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        32,
        16,
        "macros.move",
        0,
        15,
        "macros.move",
        "module Macros::macros",
        None,
    );
    // function name in macro call
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        32,
        24,
        "macros.move",
        6,
        14,
        "macros.move",
        "macro fun Macros::macros::foo($i: u64, $body: |u64| -> u64): u64",
        None,
    );
    // first non-lambda argument in macro call
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        32,
        29,
        "macros.move",
        31,
        12,
        "macros.move",
        "let p: u64",
        None,
    );
    // lambda param in second lambda argument in macro call
    assert_use_def(
        mod_symbols,
        &symbols,
        3,
        32,
        33,
        "macros.move",
        32,
        33,
        "macros.move",
        "let x: u64",
        None,
    );
    // lambda body (its param) in second lambda argument in macro call
    assert_use_def(
        mod_symbols,
        &symbols,
        4,
        32,
        36,
        "macros.move",
        32,
        33,
        "macros.move",
        "let x: u64",
        None,
    );

    // lambda in macro call containing another macro call

    // lambda param
    assert_use_def(
        mod_symbols,
        &symbols,
        5,
        37,
        49,
        "macros.move",
        37,
        49,
        "macros.move",
        "let y: u64",
        None,
    );
    // macro name in macro call in lambda body
    assert_use_def(
        mod_symbols,
        &symbols,
        7,
        37,
        68,
        "macros.move",
        6,
        14,
        "macros.move",
        "macro fun Macros::macros::foo($i: u64, $body: |u64| -> u64): u64",
        None,
    );
    // non-lambda argument nested in macro call in lambda body
    assert_use_def(
        mod_symbols,
        &symbols,
        8,
        37,
        73,
        "macros.move",
        37,
        49,
        "macros.move",
        "let y: u64",
        None,
    );
    // lambda param of lambda argument nested in macro call in lambda body
    assert_use_def(
        mod_symbols,
        &symbols,
        9,
        37,
        77,
        "macros.move",
        37,
        77,
        "macros.move",
        "let z: u64",
        None,
    );
    // lambda body (its param) of lambda argument nested in macro call in lambda body
    assert_use_def(
        mod_symbols,
        &symbols,
        10,
        37,
        80,
        "macros.move",
        37,
        77,
        "macros.move",
        "let z: u64",
        None,
    );

    // part of lambda's body in macro call that represents captured variable
    assert_use_def(
        mod_symbols,
        &symbols,
        4,
        43,
        48,
        "macros.move",
        42,
        16,
        "macros.move",
        "let mut sum: u64",
        None,
    );
    // first macro argument in macro call, receiver-syntax style
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        44,
        8,
        "macros.move",
        41,
        12,
        "macros.move",
        "let es: vector<u64>",
        None,
    );
    // aliased macro name in macro call, receiver-syntax style
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        44,
        11,
        "macros.move",
        18,
        14,
        "macros.move",
        "macro fun Macros::macros::for_each<$T>($v: &vector<$T>, $body: |&$T| -> ())",
        None,
    );

    // type parameter in macro call
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        51,
        34,
        "macros.move",
        2,
        18,
        "macros.move",
        "public struct Macros::macros::SomeStruct has drop {\n\tsome_field: u64\n}",
        Some((2, 18, "macros.move")),
    );
}
