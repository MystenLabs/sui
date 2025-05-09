// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    compiler_info::CompilerInfo,
    symbols::ide_strings::{
        abilities_to_ide_string, datatype_type_args_to_ide_string, fun_type_to_ide_string,
        mod_ident_to_ide_string, ret_type_to_ide_str, type_args_to_ide_string,
        type_list_to_ide_string, type_to_ide_string, typed_id_list_to_ide_string,
        variant_to_ide_string, visibility_to_ide_string,
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

/// Location of a use's identifier
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Copy)]
pub struct UseLoc {
    /// File where this use identifier starts
    pub fhash: FileHash,
    /// Location where this use identifier starts
    pub start: Position,
    /// Column (on the same line as start)  where this use identifier ends
    pub col_end: u32,
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
    pub col_start: u32,
    /// Column where the (use) identifier location ends on a given line
    pub col_end: u32,
    /// Location of the definition
    pub def_loc: Loc,
    /// Location of the type definition
    pub type_def_loc: Option<Loc>,
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
    pub fn new(loc: Loc) -> Self {
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
                    return Some(ChainInfo::new(chain.clone(), CT::All, false));
                }
                P::Exp_::Call(chain, _) if chain.loc.contains(&self.loc) => {
                    return Some(ChainInfo::new(chain.clone(), CT::Function, false));
                }
                P::Exp_::Pack(chain, _) if chain.loc.contains(&self.loc) => {
                    return Some(ChainInfo::new(chain.clone(), CT::Type, false));
                }
                _ => (),
            },
            CP::Binding(sp!(_, bind)) => match bind {
                P::Bind_::Unpack(chain, _) if chain.loc.contains(&self.loc) => {
                    return Some(ChainInfo::new(*(chain.clone()), CT::Type, false));
                }
                _ => (),
            },
            CP::Type(sp!(_, ty)) => match ty {
                P::Type_::Apply(chain) if chain.loc.contains(&self.loc) => {
                    return Some(ChainInfo::new(*(chain.clone()), CT::Type, false));
                }
                _ => (),
            },
            CP::Attribute(attr_val) => match &attr_val.value {
                P::AttributeValue_::ModuleAccess(chain) if chain.loc.contains(&self.loc) => {
                    return Some(ChainInfo::new(chain.clone(), CT::All, false));
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
