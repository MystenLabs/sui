// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Provides a model for a set of Move modules (and scripts, which
//! are handled like modules). The model allows to access many different aspects of the Move
//! code: all declared functions and types, their associated bytecode, their source location,
//! their source text, and the specification fragments.
//!
//! The environment is nested into a hierarchy:
//!
//! - A `GlobalEnv` which gives access to all modules plus other information on global level,
//!   and is the owner of all related data.
//! - A `ModuleEnv` which is a reference to the data of some module in the environment.
//! - A `StructEnv` which is a reference to the data of some struct in a module.
//! - A `FunctionEnv` which is a reference to the data of some function in a module.

use std::{
    any::{Any, TypeId},
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, VecDeque},
    ffi::OsStr,
    fmt::{self, Formatter},
    rc::Rc,
};

use codespan::{ByteIndex, ByteOffset, ColumnOffset, FileId, Files, LineOffset, Location, Span};
use codespan_reporting::{
    diagnostic::{Diagnostic, Label, Severity},
    term::{emit, termcolor::WriteColor, Config},
};
use itertools::Itertools;
#[allow(unused_imports)]
use log::{info, warn};
use move_ir_types::ast as IR;
use num::BigUint;

pub use move_binary_format::file_format::{AbilitySet, Visibility as FunctionVisibility};
use move_binary_format::{
    file_format::{
        AddressIdentifierIndex, Bytecode, Constant as VMConstant, ConstantPoolIndex,
        DatatypeHandleIndex, EnumDefinitionIndex, FunctionDefinition, FunctionDefinitionIndex,
        FunctionHandleIndex, FunctionInstantiation, SignatureIndex, SignatureToken,
        StructDefinitionIndex, StructFieldInformation, VariantJumpTable, Visibility,
    },
    normalized::{FunctionRef, Type as MType},
    CompiledModule,
};
use move_bytecode_source_map::{mapping::SourceMapping, source_map::SourceMap};
use move_command_line_common::files::FileHash;
use move_core_types::parsing::address::NumericalAddress;
use move_core_types::{
    account_address::AccountAddress,
    identifier::{IdentStr, Identifier},
    language_storage,
    runtime_value::MoveValue,
};
use move_disassembler::disassembler::{Disassembler, DisassemblerOptions};

use crate::{
    ast::{Attribute, ModuleName, Value},
    symbol::{Symbol, SymbolPool},
    ty::{PrimitiveType, Type, TypeDisplayContext},
};

// =================================================================================================
/// # Constants

/// A name we use to represent a script as a module.
pub const SCRIPT_MODULE_NAME: &str = "<SELF>";

/// Names used in the bytecode/AST to represent the main function of a script
pub const SCRIPT_BYTECODE_FUN_NAME: &str = "<SELF>";

/// A prefix used for structs which are backing specification ("ghost") memory.
pub const GHOST_MEMORY_PREFIX: &str = "Ghost$";

const SUI_FRAMEWORK_ADDRESS: AccountAddress = address_from_single_byte(2);

const fn address_from_single_byte(b: u8) -> AccountAddress {
    let mut addr = [0u8; AccountAddress::LENGTH];
    addr[AccountAddress::LENGTH - 1] = b;
    AccountAddress::new(addr)
}

// =================================================================================================
/// # Locations

/// A location, consisting of a FileId and a span in this file.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct Loc {
    file_id: FileId,
    span: Span,
}

impl Loc {
    pub fn new(file_id: FileId, span: Span) -> Loc {
        Loc { file_id, span }
    }

    pub fn span(&self) -> Span {
        self.span
    }

    pub fn file_id(&self) -> FileId {
        self.file_id
    }

    // Delivers a location pointing to the end of this one.
    pub fn at_end(&self) -> Loc {
        if self.span.end() > ByteIndex(0) {
            Loc::new(
                self.file_id,
                Span::new(self.span.end() - ByteOffset(1), self.span.end()),
            )
        } else {
            self.clone()
        }
    }

    // Delivers a location pointing to the start of this one.
    pub fn at_start(&self) -> Loc {
        Loc::new(
            self.file_id,
            Span::new(self.span.start(), self.span.start() + ByteOffset(1)),
        )
    }

    /// Creates a location which encloses all the locations in the provided slice,
    /// which must not be empty. All locations are expected to be in the same file.
    pub fn enclosing(locs: &[&Loc]) -> Loc {
        assert!(!locs.is_empty());
        let loc = locs[0];
        let mut start = loc.span.start();
        let mut end = loc.span.end();
        for l in locs.iter().skip(1) {
            if l.file_id() == loc.file_id() {
                start = std::cmp::min(start, l.span().start());
                end = std::cmp::max(end, l.span().end());
            }
        }
        Loc::new(loc.file_id(), Span::new(start, end))
    }

    /// Returns true if the other location is enclosed by this location.
    pub fn is_enclosing(&self, other: &Loc) -> bool {
        self.file_id == other.file_id && GlobalEnv::enclosing_span(self.span, other.span)
    }
}

impl Default for Loc {
    fn default() -> Self {
        let mut files = Files::new();
        let dummy_id = files.add(String::new(), String::new());
        Loc::new(dummy_id, Span::default())
    }
}

/// Return true if `f` is a Sui framework function declared in `module` with a name in `names`
fn is_framework_function(f: &FunctionRef, module: &str, names: Vec<&str>) -> bool {
    *f.module_id.address() == SUI_FRAMEWORK_ADDRESS
        && f.module_id.name().to_string() == module
        && names.contains(&f.function_ident.as_str())
}

/// Alias for the Loc variant of MoveIR. This uses a `&static str` instead of `FileId` for the
/// file name.
pub type MoveIrLoc = move_ir_types::location::Loc;

// =================================================================================================
/// # Identifiers
///
/// Identifiers are opaque values used to reference entities in the environment.
///
/// We have two kinds of ids: those based on an index, and those based on a symbol. We use
/// the symbol based ids where we do not have control of the definition index order in bytecode
/// (i.e. we do not know in which order move-compiler enters functions and structs into file format),
/// and index based ids where we do have control (for modules, SpecFun and SpecVar).
///
/// In any case, ids are opaque in the sense that if someone has a StructId or similar in hand,
/// it is known to be defined in the environment, as it has been obtained also from the environment.

/// Raw index type used in ids. 16 bits are sufficient currently.
pub type RawIndex = u16;

/// Identifier for a module.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub struct ModuleId(RawIndex);

/// Identifier for a named constant, relative to module.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub struct NamedConstantId(Symbol);

/// Identifier for a datatype, relative to module.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub struct DatatypeId(Symbol);

/// Identifier for an enum variant, relative to an enum.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub struct VariantId(Symbol);

/// Identifier for a field of a structure, relative to struct.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub struct FieldId(Symbol);

/// Identifier for a Move function, relative to module.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub struct FunId(Symbol);

/// Identifier for a node in the AST, relative to a module. This is used to associate attributes
/// with the node, like source location and type.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub struct NodeId(usize);

/// A global id. Instances of this type represent unique identifiers relative to `GlobalEnv`.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub struct GlobalId(usize);

/// Identifier for an intrinsic declaration, relative globally in `GlobalEnv`.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub struct IntrinsicId(usize);

/// Some identifier qualified by a module.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub struct QualifiedId<Id> {
    pub module_id: ModuleId,
    pub id: Id,
}

/// Reference type when unpacking an enum variant.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub enum RefType {
    ByValue,
    ByImmRef,
    ByMutRef,
}

/// Some identifier qualified by a module and a type instantiation.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
pub struct QualifiedInstId<Id> {
    pub module_id: ModuleId,
    pub inst: Vec<Type>,
    pub id: Id,
}

impl NamedConstantId {
    pub fn new(sym: Symbol) -> Self {
        Self(sym)
    }

    pub fn symbol(self) -> Symbol {
        self.0
    }
}

impl FunId {
    pub fn new(sym: Symbol) -> Self {
        Self(sym)
    }

    pub fn symbol(self) -> Symbol {
        self.0
    }
}

impl DatatypeId {
    pub fn new(sym: Symbol) -> Self {
        Self(sym)
    }

    pub fn symbol(self) -> Symbol {
        self.0
    }
}

impl FieldId {
    pub fn new(sym: Symbol) -> Self {
        Self(sym)
    }

    pub fn symbol(self) -> Symbol {
        self.0
    }
}

impl NodeId {
    pub fn new(idx: usize) -> Self {
        Self(idx)
    }

    pub fn as_usize(self) -> usize {
        self.0
    }
}

impl ModuleId {
    pub fn new(idx: usize) -> Self {
        Self(idx as RawIndex)
    }

    pub fn to_usize(self) -> usize {
        self.0 as usize
    }
}

impl ModuleId {
    pub fn qualified<Id>(self, id: Id) -> QualifiedId<Id> {
        QualifiedId {
            module_id: self,
            id,
        }
    }

    pub fn qualified_inst<Id>(self, id: Id, inst: Vec<Type>) -> QualifiedInstId<Id> {
        QualifiedInstId {
            module_id: self,
            inst,
            id,
        }
    }
}

impl GlobalId {
    pub fn new(idx: usize) -> Self {
        Self(idx)
    }

    pub fn as_usize(self) -> usize {
        self.0
    }
}

impl IntrinsicId {
    pub fn new(idx: usize) -> Self {
        Self(idx)
    }

    pub fn as_usize(self) -> usize {
        self.0
    }
}

impl<Id: Clone> QualifiedId<Id> {
    pub fn instantiate(self, inst: Vec<Type>) -> QualifiedInstId<Id> {
        let QualifiedId { module_id, id } = self;
        QualifiedInstId {
            module_id,
            inst,
            id,
        }
    }
}

impl<Id: Clone> QualifiedInstId<Id> {
    pub fn instantiate(self, params: &[Type]) -> Self {
        if params.is_empty() {
            self
        } else {
            let Self {
                module_id,
                inst,
                id,
            } = self;
            Self {
                module_id,
                inst: Type::instantiate_vec(inst, params),
                id,
            }
        }
    }

    pub fn instantiate_ref(&self, params: &[Type]) -> Self {
        let res = self.clone();
        res.instantiate(params)
    }

    pub fn to_qualified_id(&self) -> QualifiedId<Id> {
        let Self { module_id, id, .. } = self;
        module_id.qualified(id.to_owned())
    }
}

impl QualifiedInstId<DatatypeId> {
    pub fn to_type(&self) -> Type {
        Type::Datatype(self.module_id, self.id, self.inst.to_owned())
    }
}

// =================================================================================================
/// # Global Environment

/// Global environment for a set of modules.
#[derive(Debug)]
pub struct GlobalEnv {
    /// A Files database for the codespan crate which supports diagnostics.
    source_files: Files<String>,
    /// A mapping from file hash to file name and associated FileId. Though this information is
    /// already in `source_files`, we can't get it out of there so need to book keep here.
    file_hash_map: BTreeMap<FileHash, (String, FileId)>,
    /// A mapping from file id to associated alias map.
    file_alias_map: BTreeMap<FileId, Rc<BTreeMap<Symbol, NumericalAddress>>>,
    /// Bijective mapping between FileId and a plain int. FileId's are themselves wrappers around
    /// ints, but the inner representation is opaque and cannot be accessed. This is used so we
    /// can emit FileId's to generated code and read them back.
    file_id_to_idx: BTreeMap<FileId, u16>,
    file_idx_to_id: BTreeMap<u16, FileId>,
    /// A set indicating whether a file id is a target or a dependency.
    file_id_is_dep: BTreeSet<FileId>,
    /// A special constant location representing an unknown location.
    /// This uses a pseudo entry in `source_files` to be safely represented.
    unknown_loc: Loc,
    /// An equivalent of the MoveIrLoc to the above location. Used to map back and force between
    /// them.
    unknown_move_ir_loc: MoveIrLoc,
    /// A special constant location representing an opaque location.
    /// In difference to an `unknown_loc`, this is a well-known but undisclosed location.
    internal_loc: Loc,
    /// Accumulated diagnosis. In a RefCell so we can add to it without needing a mutable GlobalEnv.
    /// The boolean indicates whether the diag was reported.
    diags: RefCell<Vec<(Diagnostic<FileId>, bool)>>,
    /// Pool of symbols -- internalized strings.
    symbol_pool: SymbolPool,
    /// A counter for allocating node ids.
    next_free_node_id: RefCell<usize>,
    /// A map from node id to associated information of the expression.
    exp_info: RefCell<BTreeMap<NodeId, ExpInfo>>,
    /// List of loaded modules, in order they have been provided using `add`.
    pub module_data: Vec<ModuleData>,
    /// A type-indexed container for storing extension data in the environment.
    extensions: RefCell<BTreeMap<TypeId, Box<dyn Any>>>,
    /// The address of the standard and extension libaries.
    stdlib_address: Option<BigUint>,
    extlib_address: Option<BigUint>,
}

/// Struct a helper type for implementing fmt::Display depending on GlobalEnv
pub struct EnvDisplay<'a, T> {
    pub env: &'a GlobalEnv,
    pub val: &'a T,
}

impl GlobalEnv {
    /// Creates a new environment.
    pub fn new() -> Self {
        let mut source_files = Files::new();
        let mut file_hash_map = BTreeMap::new();
        let mut file_id_to_idx = BTreeMap::new();
        let mut file_idx_to_id = BTreeMap::new();
        let mut fake_loc = |content: &str| {
            let file_id = source_files.add(content, content.to_string());
            let file_hash = FileHash::new(content);
            file_hash_map.insert(file_hash, (content.to_string(), file_id));
            let file_idx = file_id_to_idx.len() as u16;
            file_id_to_idx.insert(file_id, file_idx);
            file_idx_to_id.insert(file_idx, file_id);
            Loc::new(
                file_id,
                Span::from(ByteIndex(0_u32)..ByteIndex(content.len() as u32)),
            )
        };
        let unknown_loc = fake_loc("<unknown>");
        let unknown_move_ir_loc = MoveIrLoc::new(FileHash::new("<unknown>"), 0, 0);
        let internal_loc = fake_loc("<internal>");
        GlobalEnv {
            source_files,
            unknown_loc,
            unknown_move_ir_loc,
            internal_loc,
            file_hash_map,
            file_alias_map: BTreeMap::new(),
            file_id_to_idx,
            file_idx_to_id,
            file_id_is_dep: BTreeSet::new(),
            diags: RefCell::new(vec![]),
            symbol_pool: SymbolPool::new(),
            next_free_node_id: Default::default(),
            exp_info: Default::default(),
            module_data: vec![],
            extensions: Default::default(),
            stdlib_address: None,
            extlib_address: None,
        }
    }

    /// Creates a display container for the given value. There must be an implementation
    /// of fmt::Display for an instance to work in formatting.
    pub fn display<'a, T>(&'a self, val: &'a T) -> EnvDisplay<'a, T> {
        EnvDisplay { env: self, val }
    }

    /// Stores extension data in the environment. This can be arbitrary data which is
    /// indexed by type. Used by tools which want to store their own data in the environment,
    /// like a set of tool dependent options/flags. This can also be used to update
    /// extension data.
    pub fn set_extension<T: Any>(&self, x: T) {
        let id = TypeId::of::<T>();
        self.extensions
            .borrow_mut()
            .insert(id, Box::new(Rc::new(x)));
    }

    /// Retrieves extension data from the environment. Use as in `env.get_extension::<T>()`.
    /// An Rc<T> is returned because extension data is stored in a RefCell and we can't use
    /// lifetimes (`&'a T`) to control borrowing.
    pub fn get_extension<T: Any>(&self) -> Option<Rc<T>> {
        let id = TypeId::of::<T>();
        self.extensions
            .borrow()
            .get(&id)
            .and_then(|d| d.downcast_ref::<Rc<T>>().cloned())
    }

    /// Retrieves a clone of the extension data from the environment. Use as in `env.get_cloned_extension::<T>()`.
    pub fn get_cloned_extension<T: Any + Clone>(&self) -> T {
        let id = TypeId::of::<T>();
        let d = self
            .extensions
            .borrow_mut()
            .remove(&id)
            .expect("extension defined")
            .downcast_ref::<Rc<T>>()
            .cloned()
            .unwrap();
        Rc::try_unwrap(d).unwrap_or_else(|d| d.as_ref().clone())
    }

    /// Updates extension data. If they are no outstanding references to this extension it
    /// is updated in place, otherwise it will be cloned before the update.
    pub fn update_extension<T: Any + Clone>(&self, f: impl FnOnce(&mut T)) {
        let id = TypeId::of::<T>();
        let d = self
            .extensions
            .borrow_mut()
            .remove(&id)
            .expect("extension defined")
            .downcast_ref::<Rc<T>>()
            .cloned()
            .unwrap();
        let mut curr = Rc::try_unwrap(d).unwrap_or_else(|d| d.as_ref().clone());
        f(&mut curr);
        self.set_extension(curr);
    }

    /// Checks whether there is an extension of type `T`.
    pub fn has_extension<T: Any>(&self) -> bool {
        let id = TypeId::of::<T>();
        self.extensions.borrow().contains_key(&id)
    }

    /// Clear extension data from the environment (return the data if it is previously set).
    /// Use as in `env.clear_extension::<T>()` and an `Rc<T>` is returned if the extension data is
    /// previously stored in the environment.
    pub fn clear_extension<T: Any>(&self) -> Option<Rc<T>> {
        let id = TypeId::of::<T>();
        self.extensions
            .borrow_mut()
            .remove(&id)
            .and_then(|d| d.downcast::<Rc<T>>().ok())
            .map(|boxed| *boxed)
    }

    /// Returns a reference to the symbol pool owned by this environment.
    pub fn symbol_pool(&self) -> &SymbolPool {
        &self.symbol_pool
    }

    /// Adds a source to this environment, returning a FileId for it.
    pub fn add_source(
        &mut self,
        file_hash: FileHash,
        address_aliases: Rc<BTreeMap<Symbol, NumericalAddress>>,
        file_name: &str,
        source: &str,
        is_dep: bool,
    ) -> FileId {
        let file_id = self.source_files.add(file_name, source.to_string());
        self.stdlib_address =
            self.resolve_std_address_alias(self.stdlib_address.clone(), "std", &address_aliases);
        self.extlib_address = self.resolve_std_address_alias(
            self.extlib_address.clone(),
            "Extensions",
            &address_aliases,
        );
        self.file_alias_map.insert(file_id, address_aliases);
        self.file_hash_map
            .insert(file_hash, (file_name.to_string(), file_id));
        let file_idx = self.file_id_to_idx.len() as u16;
        self.file_id_to_idx.insert(file_id, file_idx);
        self.file_idx_to_id.insert(file_idx, file_id);
        if is_dep {
            self.file_id_is_dep.insert(file_id);
        }
        file_id
    }

    fn resolve_std_address_alias(
        &self,
        def: Option<BigUint>,
        name: &str,
        aliases: &BTreeMap<Symbol, NumericalAddress>,
    ) -> Option<BigUint> {
        let name_sym = self.symbol_pool().make(name);
        if let Some(a) = aliases.get(&name_sym) {
            let addr = BigUint::from_bytes_be(a.as_ref());
            if matches!(&def, Some(other_addr) if &addr != other_addr) {
                self.error(
                    &self.unknown_loc,
                    &format!(
                        "Ambiguous definition of standard address alias `{}` (`0x{} != 0x{}`).\
                                 This alias currently must be unique across all packages.",
                        name,
                        addr,
                        def.unwrap()
                    ),
                );
            }
            Some(addr)
        } else {
            def
        }
    }

    /// Find all target modules and return in a vector
    pub fn get_target_modules(&self) -> Vec<ModuleEnv> {
        let mut target_modules: Vec<ModuleEnv> = vec![];
        for module_env in self.get_modules() {
            if module_env.is_target() {
                target_modules.push(module_env);
            }
        }
        target_modules
    }

    /// Adds diagnostic to the environment.
    pub fn add_diag(&self, diag: Diagnostic<FileId>) {
        self.diags.borrow_mut().push((diag, false));
    }

    /// Adds an error to this environment, without notes.
    pub fn error(&self, loc: &Loc, msg: &str) {
        self.diag(Severity::Error, loc, msg)
    }

    /// Adds an error to this environment, with notes.
    pub fn error_with_notes(&self, loc: &Loc, msg: &str, notes: Vec<String>) {
        self.diag_with_notes(Severity::Error, loc, msg, notes)
    }

    /// Adds a diagnostic of given severity to this environment.
    pub fn diag(&self, severity: Severity, loc: &Loc, msg: &str) {
        let diag = Diagnostic::new(severity)
            .with_message(msg)
            .with_labels(vec![Label::primary(loc.file_id, loc.span)]);
        self.add_diag(diag);
    }

    /// Adds a diagnostic of given severity to this environment, with notes.
    pub fn diag_with_notes(&self, severity: Severity, loc: &Loc, msg: &str, notes: Vec<String>) {
        let diag = Diagnostic::new(severity)
            .with_message(msg)
            .with_labels(vec![Label::primary(loc.file_id, loc.span)]);
        let diag = diag.with_notes(notes);
        self.add_diag(diag);
    }

    /// Adds a diagnostic of given severity to this environment, with secondary labels.
    pub fn diag_with_labels(
        &self,
        severity: Severity,
        loc: &Loc,
        msg: &str,
        labels: Vec<(Loc, String)>,
    ) {
        let diag = Diagnostic::new(severity)
            .with_message(msg)
            .with_labels(vec![Label::primary(loc.file_id, loc.span)]);
        let labels = labels
            .into_iter()
            .map(|(l, m)| Label::secondary(l.file_id, l.span).with_message(m))
            .collect_vec();
        let diag = diag.with_labels(labels);
        self.add_diag(diag);
    }

    /// Checks whether any of the diagnostics contains string.
    pub fn has_diag(&self, pattern: &str) -> bool {
        self.diags
            .borrow()
            .iter()
            .any(|(d, _)| d.message.contains(pattern))
    }

    /// Clear all accumulated diagnosis.
    pub fn clear_diag(&self) {
        self.diags.borrow_mut().clear();
    }

    /// Returns the unknown location.
    pub fn unknown_loc(&self) -> Loc {
        self.unknown_loc.clone()
    }

    /// Returns a Move IR version of the unknown location which is guaranteed to map to the
    /// regular unknown location via `to_loc`.
    pub fn unknown_move_ir_loc(&self) -> MoveIrLoc {
        self.unknown_move_ir_loc
    }

    /// Returns the internal location.
    pub fn internal_loc(&self) -> Loc {
        self.internal_loc.clone()
    }

    /// Converts a Loc as used by the move-compiler compiler to the one we are using here.
    /// TODO: move-compiler should use FileId as well so we don't need this here. There is already
    /// a todo in their code to remove the current use of `&'static str` for file names in Loc.
    pub fn to_loc(&self, loc: &MoveIrLoc) -> Loc {
        let Some(file_id) = self.get_file_id(loc.file_hash()) else {
            return self.unknown_loc();
        };
        Loc {
            file_id,
            span: Span::new(loc.start(), loc.end()),
        }
    }

    /// Returns the file id for a file name, if defined.
    pub fn get_file_id(&self, fhash: FileHash) -> Option<FileId> {
        self.file_hash_map.get(&fhash).map(|(_, id)| id).cloned()
    }

    /// Maps a FileId to an index which can be mapped back to a FileId.
    pub fn file_id_to_idx(&self, file_id: FileId) -> u16 {
        *self
            .file_id_to_idx
            .get(&file_id)
            .expect("file_id undefined")
    }

    /// Maps an index which was obtained by `file_id_to_idx` back to a FileId.
    pub fn file_idx_to_id(&self, file_idx: u16) -> FileId {
        *self
            .file_idx_to_id
            .get(&file_idx)
            .expect("file_idx undefined")
    }

    /// Returns file name and line/column position for a location, if available.
    pub fn get_file_and_location(&self, loc: &Loc) -> Option<(String, Location)> {
        self.get_location(loc).map(|line_column| {
            (
                self.source_files
                    .name(loc.file_id())
                    .to_string_lossy()
                    .to_string(),
                line_column,
            )
        })
    }

    /// Returns line/column position for a location, if available.
    pub fn get_location(&self, loc: &Loc) -> Option<Location> {
        self.source_files
            .location(loc.file_id(), loc.span().start())
            .ok()
    }

    /// Return the source text for the given location.
    pub fn get_source(&self, loc: &Loc) -> Result<&str, codespan_reporting::files::Error> {
        self.source_files.source_slice(loc.file_id, loc.span)
    }

    /// Return the source file name for `file_id`
    pub fn get_file(&self, file_id: FileId) -> &OsStr {
        self.source_files.name(file_id)
    }

    /// Return the source file names.
    pub fn get_source_file_names(&self) -> Vec<String> {
        self.file_hash_map
            .iter()
            .filter_map(|(_, (k, _))| {
                if k.eq("<internal>") || k.eq("<unknown>") {
                    None
                } else {
                    Some(k.clone())
                }
            })
            .collect()
    }

    /// Return the source file ids.
    pub fn get_source_file_ids(&self) -> Vec<FileId> {
        self.file_hash_map
            .iter()
            .filter_map(|(_, (k, id))| {
                if k.eq("<internal>") || k.eq("<unknown>") {
                    None
                } else {
                    Some(*id)
                }
            })
            .collect()
    }

    // Gets the number of source files in this environment.
    pub fn get_file_count(&self) -> usize {
        self.file_hash_map.len()
    }

    /// Returns true if diagnostics have error severity or worse.
    pub fn has_errors(&self) -> bool {
        self.error_count() > 0
    }

    /// Returns the number of diagnostics.
    pub fn diag_count(&self, min_severity: Severity) -> usize {
        self.diags
            .borrow()
            .iter()
            .filter(|(d, _)| d.severity >= min_severity)
            .count()
    }

    /// Returns the number of errors.
    pub fn error_count(&self) -> usize {
        self.diag_count(Severity::Error)
    }

    /// Returns true if diagnostics have warning severity or worse.
    pub fn has_warnings(&self) -> bool {
        self.diags
            .borrow()
            .iter()
            .any(|(d, _)| d.severity >= Severity::Warning)
    }

    /// Writes accumulated diagnostics of given or higher severity.
    pub fn report_diag<W: WriteColor>(&self, writer: &mut W, severity: Severity) {
        self.report_diag_with_filter(writer, |d| d.severity >= severity)
    }

    /// Writes accumulated diagnostics that pass through `filter`
    pub fn report_diag_with_filter<W: WriteColor, F: Fn(&Diagnostic<FileId>) -> bool>(
        &self,
        writer: &mut W,
        filter: F,
    ) {
        let mut shown = BTreeSet::new();
        for (diag, reported) in self
            .diags
            .borrow_mut()
            .iter_mut()
            .filter(|(d, _)| filter(d))
        {
            if !*reported {
                // Avoid showing the same message twice. This can happen e.g. because of
                // duplication of expressions via schema inclusion.
                if shown.insert(format!("{:?}", diag)) {
                    emit(writer, &Config::default(), &self.source_files, diag)
                        .expect("emit must not fail");
                }
                *reported = true;
            }
        }
    }

    /// Adds a new module to the environment. StructData and FunctionData need to be provided
    /// in definition index order. See `create_function_data` and `create_struct_data` for how
    /// to create them.
    #[allow(clippy::too_many_arguments)]
    pub fn add(
        &mut self,
        loc: Loc,
        attributes: Vec<Attribute>,
        module: CompiledModule,
        source_map: SourceMap,
        named_constants: BTreeMap<NamedConstantId, NamedConstantData>,
        struct_data: BTreeMap<DatatypeId, StructData>,
        enum_data: BTreeMap<DatatypeId, EnumData>,
        function_data: BTreeMap<FunId, FunctionData>,
    ) {
        let idx = self.module_data.len();
        let effective_name = if module.self_id().name().as_str() == SCRIPT_MODULE_NAME {
            // Use the name of the first function in this module.
            function_data
                .iter()
                .next()
                .expect("functions in script")
                .1
                .name
        } else {
            self.symbol_pool.make(module.self_id().name().as_str())
        };
        let name = ModuleName::from_str(&module.self_id().address().to_string(), effective_name);
        let struct_idx_to_id: BTreeMap<StructDefinitionIndex, DatatypeId> = struct_data
            .iter()
            .map(|(id, data)| match &data.info {
                StructInfo::Declared { def_idx, .. } => (*def_idx, *id),
            })
            .collect();
        let function_idx_to_id: BTreeMap<FunctionDefinitionIndex, FunId> = function_data
            .iter()
            .map(|(id, data)| (data.def_idx, *id))
            .collect();

        let enum_idx_to_id: BTreeMap<EnumDefinitionIndex, DatatypeId> = enum_data
            .iter()
            .map(|(id, data)| (data.def_idx, *id))
            .collect();

        self.module_data.push(ModuleData {
            name,
            id: ModuleId(idx as RawIndex),
            module,
            named_constants,
            struct_data,
            struct_idx_to_id,
            enum_data,
            enum_idx_to_id,
            function_data,
            function_idx_to_id,
            source_map,
            loc,
            attributes,
            used_modules: Default::default(),
            friend_modules: Default::default(),
        });
    }

    /// Creates data for a named constant.
    pub fn create_named_constant_data(
        &self,
        name: Symbol,
        loc: Loc,
        typ: Type,
        value: Value,
        attributes: Vec<Attribute>,
    ) -> NamedConstantData {
        NamedConstantData {
            name,
            loc,
            typ,
            value,
            attributes,
        }
    }

    /// Creates data for a function, adding any information not contained in bytecode. This is
    /// a helper for adding a new module to the environment.
    pub fn create_function_data(
        &self,
        module: &CompiledModule,
        def_idx: FunctionDefinitionIndex,
        name: Symbol,
        loc: Loc,
        attributes: Vec<Attribute>,
        arg_names: Vec<Symbol>,
        type_arg_names: Vec<Symbol>,
    ) -> FunctionData {
        let handle_idx = module.function_def_at(def_idx).function;
        FunctionData {
            name,
            loc,
            attributes,
            def_idx,
            handle_idx,
            arg_names,
            type_arg_names,
            called_funs: Default::default(),
            calling_funs: Default::default(),
            transitive_closure_of_called_funs: Default::default(),
        }
    }

    /// Creates data for a struct declared in Move. Currently all information is contained in
    /// the byte code. This is a helper for adding a new module to the environment.
    pub fn create_move_struct_data(
        &self,
        module: &CompiledModule,
        def_idx: StructDefinitionIndex,
        name: Symbol,
        loc: Loc,
        attributes: Vec<Attribute>,
    ) -> StructData {
        let handle_idx = module.struct_def_at(def_idx).struct_handle;
        let field_data = if let StructFieldInformation::Declared(fields) =
            &module.struct_def_at(def_idx).field_information
        {
            let mut map = BTreeMap::new();
            for (offset, field) in fields.iter().enumerate() {
                let name = self
                    .symbol_pool
                    .make(module.identifier_at(field.name).as_str());
                let info = FieldInfo::DeclaredStruct { def_idx };
                map.insert(FieldId(name), FieldData { name, offset, info });
            }
            map
        } else {
            BTreeMap::new()
        };
        let info = StructInfo::Declared {
            def_idx,
            handle_idx,
        };
        StructData {
            name,
            loc,
            attributes,
            info,
            field_data,
        }
    }

    /// Creates data for a enum declared in Move. Currently all information is contained in
    /// the byte code. This is a helper for adding a new module to the environment.
    pub fn create_move_enum_data(
        &self,
        module: &CompiledModule,
        def_idx: EnumDefinitionIndex,
        name: Symbol,
        loc: Loc,
        source_map: Option<&SourceMap>,
        attributes: Vec<Attribute>,
    ) -> EnumData {
        let enum_def = module.enum_def_at(def_idx);
        let enum_smap = source_map.map(|smap| smap.get_enum_source_map(def_idx).unwrap());
        let handle_idx = enum_def.enum_handle;
        let mut variant_data = BTreeMap::new();
        for (tag, variant) in enum_def.variants.iter().enumerate() {
            let mut field_data = BTreeMap::new();
            for (offset, field) in variant.fields.iter().enumerate() {
                let name = self
                    .symbol_pool
                    .make(module.identifier_at(field.name).as_str());
                let info = FieldInfo::DeclaredEnum { def_idx };
                field_data.insert(FieldId(name), FieldData { name, offset, info });
            }
            let variant_name = self
                .symbol_pool
                .make(module.identifier_at(variant.variant_name).as_str());
            let loc = match enum_smap {
                None => Loc::default(),
                Some(smap) => self.to_loc(&smap.variants[tag].0 .1),
            };
            variant_data.insert(
                VariantId(variant_name),
                VariantData {
                    name: variant_name,
                    loc,
                    tag,
                    field_data,
                },
            );
        }

        EnumData {
            name,
            loc,
            attributes,
            def_idx,
            handle_idx,
            variant_data,
        }
    }

    /// Return the name of the ghost memory associated with spec var.
    pub fn ghost_memory_name(&self, spec_var_name: Symbol) -> Symbol {
        self.symbol_pool.make(&format!(
            "{}{}",
            GHOST_MEMORY_PREFIX,
            self.symbol_pool.string(spec_var_name)
        ))
    }

    /// Finds a module by name and returns an environment for it.
    pub fn find_module(&self, name: &ModuleName) -> Option<ModuleEnv<'_>> {
        for module_data in &self.module_data {
            let module_env = ModuleEnv {
                env: self,
                data: module_data,
            };
            if module_env.get_name() == name {
                return Some(module_env);
            }
        }
        None
    }

    /// Finds a module by simple name and returns an environment for it.
    /// TODO: we may need to disallow this to support modules of the same simple name but with
    ///    different addresses in one verification session.
    pub fn find_module_by_name(&self, simple_name: Symbol) -> Option<ModuleEnv<'_>> {
        self.get_modules()
            .find(|m| m.get_name().name() == simple_name)
    }

    /// Find a module by its bytecode format ID
    pub fn find_module_by_language_storage_id(
        &self,
        id: &language_storage::ModuleId,
    ) -> Option<ModuleEnv<'_>> {
        self.find_module(&self.to_module_name(id))
    }

    /// Find a function by its bytecode format name and ID
    pub fn find_function_by_language_storage_id_name(
        &self,
        id: &language_storage::ModuleId,
        name: &IdentStr,
    ) -> Option<FunctionEnv<'_>> {
        self.find_module_by_language_storage_id(id)
            .and_then(|menv| menv.find_function(menv.symbol_pool().make(name.as_str())))
    }

    /// Gets a StructEnv in this module by its `StructTag`
    pub fn find_datatype_by_tag(
        &self,
        tag: &language_storage::StructTag,
    ) -> Option<QualifiedId<DatatypeId>> {
        self.find_module(&self.to_module_name(&tag.module_id()))
            .and_then(|menv| {
                menv.find_struct_by_identifier(tag.name.clone())
                    .map(|sid| menv.get_id().qualified(sid))
                    .or_else(|| {
                        menv.find_enum_by_identifier(tag.name.clone())
                            .map(|sid| menv.get_id().qualified(sid))
                    })
            })
    }

    /// Return the module enclosing this location.
    pub fn get_enclosing_module(&self, loc: &Loc) -> Option<ModuleEnv<'_>> {
        for data in &self.module_data {
            if data.loc.file_id() == loc.file_id()
                && Self::enclosing_span(data.loc.span(), loc.span())
            {
                return Some(ModuleEnv { env: self, data });
            }
        }
        None
    }

    /// Returns the function enclosing this location.
    pub fn get_enclosing_function(&self, loc: &Loc) -> Option<FunctionEnv<'_>> {
        // Currently we do a brute-force linear search, may need to speed this up if it appears
        // to be a bottleneck.
        let module_env = self.get_enclosing_module(loc)?;
        for func_env in module_env.into_functions() {
            if Self::enclosing_span(func_env.get_loc().span(), loc.span()) {
                return Some(func_env.clone());
            }
        }
        None
    }

    /// Returns the struct enclosing this location.
    pub fn get_enclosing_struct(&self, loc: &Loc) -> Option<StructEnv<'_>> {
        let module_env = self.get_enclosing_module(loc)?;
        module_env
            .into_structs()
            .find(|struct_env| Self::enclosing_span(struct_env.get_loc().span(), loc.span()))
    }

    fn enclosing_span(outer: Span, inner: Span) -> bool {
        inner.start() >= outer.start() && inner.end() <= outer.end()
    }

    /// Return the `FunctionEnv` for `fun`
    pub fn get_function(&self, fun: QualifiedId<FunId>) -> FunctionEnv<'_> {
        self.get_module(fun.module_id).into_function(fun.id)
    }

    /// Return the `StructEnv` for `str`
    pub fn get_struct(&self, str: QualifiedId<DatatypeId>) -> StructEnv<'_> {
        self.get_module(str.module_id).into_struct(str.id)
    }

    // Gets the number of modules in this environment.
    pub fn get_module_count(&self) -> usize {
        self.module_data.len()
    }

    /// Gets a module by id.
    pub fn get_module(&self, id: ModuleId) -> ModuleEnv<'_> {
        let module_data = &self.module_data[id.0 as usize];
        ModuleEnv {
            env: self,
            data: module_data,
        }
    }

    /// Gets a struct by qualified id.
    pub fn get_struct_qid(&self, qid: QualifiedId<DatatypeId>) -> StructEnv<'_> {
        self.get_module(qid.module_id).into_struct(qid.id)
    }

    /// Gets a function by qualified id.
    pub fn get_function_qid(&self, qid: QualifiedId<FunId>) -> FunctionEnv<'_> {
        self.get_module(qid.module_id).into_function(qid.id)
    }

    /// Returns an iterator for all modules in the environment.
    pub fn get_modules(&self) -> impl Iterator<Item = ModuleEnv<'_>> {
        self.module_data.iter().map(move |module_data| ModuleEnv {
            env: self,
            data: module_data,
        })
    }

    /// Returns an iterator for all bytecode modules in the environment.
    pub fn get_bytecode_modules(&self) -> impl Iterator<Item = &CompiledModule> {
        self.module_data
            .iter()
            .map(|module_data| &module_data.module)
    }

    /// Converts a storage module id into an AST module name.
    pub fn to_module_name(&self, storage_id: &language_storage::ModuleId) -> ModuleName {
        ModuleName::from_str(
            &storage_id.address().to_string(),
            self.symbol_pool.make(storage_id.name().as_str()),
        )
    }

    /// Attempt to compute a struct tag for (`mid`, `sid`, `ts`). Returns `Some` if all types in
    /// `ts` are closed, `None` otherwise
    pub fn get_struct_tag(
        &self,
        mid: ModuleId,
        sid: DatatypeId,
        ts: &[Type],
    ) -> Option<language_storage::StructTag> {
        self.get_datatype(mid, sid, ts)?.into_struct_tag()
    }

    /// Attempt to compute a struct type for (`mid`, `sid`, `ts`).
    pub fn get_datatype(&self, mid: ModuleId, sid: DatatypeId, ts: &[Type]) -> Option<MType> {
        let menv = self.get_module(mid);
        let name = menv
            .find_struct(sid.symbol())
            .map(|senv| senv.get_identifier())
            .or_else(|| {
                menv.find_enum(sid.symbol())
                    .map(|eenv| eenv.get_identifier())
            })??;
        Some(MType::Struct {
            address: *menv.self_address(),
            module: menv.get_identifier(),
            name,
            type_arguments: ts
                .iter()
                .map(|t| t.clone().into_normalized_type(self).unwrap())
                .collect(),
        })
    }

    /// Gets the location of the given node.
    pub fn get_node_loc(&self, node_id: NodeId) -> Loc {
        self.exp_info
            .borrow()
            .get(&node_id)
            .map_or_else(|| self.unknown_loc(), |info| info.loc.clone())
    }

    /// Gets the type of the given node.
    pub fn get_node_type(&self, node_id: NodeId) -> Type {
        self.get_node_type_opt(node_id).expect("node type defined")
    }

    /// Gets the type of the given node, if available.
    pub fn get_node_type_opt(&self, node_id: NodeId) -> Option<Type> {
        self.exp_info
            .borrow()
            .get(&node_id)
            .map(|info| info.ty.clone())
    }

    /// Converts an index into a node id.
    pub fn index_to_node_id(&self, index: usize) -> Option<NodeId> {
        let id = NodeId::new(index);
        if self.exp_info.borrow().get(&id).is_some() {
            Some(id)
        } else {
            None
        }
    }

    /// Returns the next free node number.
    pub fn next_free_node_number(&self) -> usize {
        *self.next_free_node_id.borrow()
    }

    /// Allocates a new node id.
    pub fn new_node_id(&self) -> NodeId {
        let id = NodeId::new(*self.next_free_node_id.borrow());
        let mut r = self.next_free_node_id.borrow_mut();
        *r = r.checked_add(1).expect("NodeId overflow");
        id
    }

    /// Allocates a new node id and assigns location and type to it.
    pub fn new_node(&self, loc: Loc, ty: Type) -> NodeId {
        let id = self.new_node_id();
        self.exp_info.borrow_mut().insert(id, ExpInfo::new(loc, ty));
        id
    }

    /// Updates type for the given node id. Must have been set before.
    pub fn update_node_type(&self, node_id: NodeId, ty: Type) {
        let mut mods = self.exp_info.borrow_mut();
        let info = mods.get_mut(&node_id).expect("node exist");
        info.ty = ty;
    }

    /// Sets instantiation for the given node id. Must not have been set before.
    pub fn set_node_instantiation(&self, node_id: NodeId, instantiation: Vec<Type>) {
        let mut mods = self.exp_info.borrow_mut();
        let info = mods.get_mut(&node_id).expect("node exist");
        assert!(info.instantiation.is_none());
        info.instantiation = Some(instantiation);
    }

    /// Updates instantiation for the given node id. Must have been set before.
    pub fn update_node_instantiation(&self, node_id: NodeId, instantiation: Vec<Type>) {
        let mut mods = self.exp_info.borrow_mut();
        let info = mods.get_mut(&node_id).expect("node exist");
        assert!(info.instantiation.is_some());
        info.instantiation = Some(instantiation);
    }

    /// Gets the type parameter instantiation associated with the given node.
    pub fn get_node_instantiation(&self, node_id: NodeId) -> Vec<Type> {
        self.get_node_instantiation_opt(node_id).unwrap_or_default()
    }

    /// Gets the type parameter instantiation associated with the given node, if it is available.
    pub fn get_node_instantiation_opt(&self, node_id: NodeId) -> Option<Vec<Type>> {
        self.exp_info
            .borrow()
            .get(&node_id)
            .and_then(|info| info.instantiation.clone())
    }

    /// Gets the type parameter instantiation associated with the given node, if it is available.
    pub fn get_nodes(&self) -> Vec<NodeId> {
        (*self.exp_info.borrow()).clone().into_keys().collect_vec()
    }

    /// Return the total number of declared functions in the modules of `self`
    pub fn get_declared_function_count(&self) -> usize {
        let mut total = 0;
        for m in &self.module_data {
            total += m.module.function_defs().len();
        }
        total
    }

    /// Return the total number of declared structs in the modules of `self`
    pub fn get_declared_struct_count(&self) -> usize {
        let mut total = 0;
        for m in &self.module_data {
            total += m.module.struct_defs().len();
        }
        total
    }

    /// Return the total number of Move bytecode instructions (not stackless bytecode) in the modules of `self`
    pub fn get_move_bytecode_instruction_count(&self) -> usize {
        let mut total = 0;
        for m in self.get_modules() {
            for f in m.get_functions() {
                total += f.get_bytecode().len();
            }
        }
        total
    }

    /// Produce a TypeDisplayContext to print types within the scope of this env
    pub fn get_type_display_ctx(&self) -> TypeDisplayContext {
        TypeDisplayContext::WithEnv {
            env: self,
            type_param_names: None,
        }
    }

    /// Returns the address where the standard lib is defined.
    pub fn get_stdlib_address(&self) -> BigUint {
        self.stdlib_address.clone().unwrap_or_else(|| 1u16.into())
    }

    /// Returns the address where the extensions libs are defined.
    pub fn get_extlib_address(&self) -> BigUint {
        self.extlib_address.clone().unwrap_or_else(|| 2u16.into())
    }
}

impl Default for GlobalEnv {
    fn default() -> Self {
        Self::new()
    }
}

// =================================================================================================
/// # Module Environment

/// Represents data for a module.
#[derive(Debug)]
pub struct ModuleData {
    /// Module name.
    pub name: ModuleName,

    /// Id of this module in the global env.
    pub id: ModuleId,

    /// Attributes attached to this module.
    attributes: Vec<Attribute>,

    /// Module byte code.
    pub module: CompiledModule,

    /// Named constant data
    pub named_constants: BTreeMap<NamedConstantId, NamedConstantData>,

    /// Struct data.
    pub struct_data: BTreeMap<DatatypeId, StructData>,

    /// Enum data.
    pub enum_data: BTreeMap<DatatypeId, EnumData>,

    /// Mapping from struct definition index to id in struct map.
    pub struct_idx_to_id: BTreeMap<StructDefinitionIndex, DatatypeId>,

    /// Mapping from enum definition index to id in the enum_data map
    pub enum_idx_to_id: BTreeMap<EnumDefinitionIndex, DatatypeId>,

    /// Function data.
    pub function_data: BTreeMap<FunId, FunctionData>,

    /// Mapping from function definition index to id in above map.
    pub function_idx_to_id: BTreeMap<FunctionDefinitionIndex, FunId>,

    /// Module source location information.
    pub source_map: SourceMap,

    /// The location of this module.
    pub loc: Loc,

    /// A cache for the modules used by this one.
    used_modules: RefCell<BTreeMap<bool, BTreeSet<ModuleId>>>,

    /// A cache for the modules declared as friends by this one.
    friend_modules: RefCell<Option<BTreeSet<ModuleId>>>,
}

impl ModuleData {
    pub fn stub(name: ModuleName, id: ModuleId, module: CompiledModule) -> Self {
        let ident = IR::ModuleIdent::new(
            IR::ModuleName(module.name().as_str().into()),
            *module.address(),
        );
        ModuleData {
            name,
            id,
            module,
            named_constants: BTreeMap::new(),
            struct_data: BTreeMap::new(),
            struct_idx_to_id: BTreeMap::new(),
            function_data: BTreeMap::new(),
            function_idx_to_id: BTreeMap::new(),
            source_map: SourceMap::new(MoveIrLoc::new(FileHash::empty(), 0, 0), ident),
            loc: Loc::default(),
            attributes: Default::default(),
            used_modules: Default::default(),
            friend_modules: Default::default(),
            enum_data: BTreeMap::new(),
            enum_idx_to_id: BTreeMap::new(),
        }
    }
}

/// Represents a module environment.
#[derive(Debug, Clone)]
pub struct ModuleEnv<'env> {
    /// Reference to the outer env.
    pub env: &'env GlobalEnv,

    /// Reference to the data of the module.
    data: &'env ModuleData,
}

impl<'env> ModuleEnv<'env> {
    /// Returns the id of this module in the global env.
    pub fn get_id(&self) -> ModuleId {
        self.data.id
    }

    /// Returns the name of this module.
    pub fn get_name(&'env self) -> &'env ModuleName {
        &self.data.name
    }

    /// Returns true if either the full name or simple name of this module matches the given string
    pub fn matches_name(&self, name: &str) -> bool {
        self.get_full_name_str() == name
            || self.get_name().display(self.symbol_pool()).to_string() == name
    }

    /// Returns the location of this module.
    pub fn get_loc(&'env self) -> Loc {
        self.data.loc.clone()
    }

    /// Returns the attributes of this module.
    pub fn get_attributes(&self) -> &[Attribute] {
        &self.data.attributes
    }

    /// Returns full name as a string.
    pub fn get_full_name_str(&self) -> String {
        self.get_name().display_full(self.symbol_pool()).to_string()
    }

    /// Returns the VM identifier for this module
    pub fn get_identifier(&'env self) -> Identifier {
        self.data.module.name().to_owned()
    }

    /// Returns true if this is a module representing a script.
    pub fn is_script_module(&self) -> bool {
        self.data.name.is_script()
    }

    /// Returns true of this module is target of compilation. A non-target module is
    /// a dependency only but not explicitly requested to process.
    pub fn is_target(&self) -> bool {
        let file_id = self.data.loc.file_id;
        !self.env.file_id_is_dep.contains(&file_id)
    }

    /// Returns the path to source file of this module.
    pub fn get_source_path(&self) -> &OsStr {
        let file_id = self.data.loc.file_id;
        self.env.source_files.name(file_id)
    }

    /// Return the set of language storage ModuleId's that this module's bytecode depends on
    /// (including itself), friend modules are excluded from the return result.
    pub fn get_dependencies(&self) -> Vec<language_storage::ModuleId> {
        let compiled_module = &self.data.module;
        let mut deps = compiled_module.immediate_dependencies();
        deps.push(compiled_module.self_id());
        deps
    }

    /// Return the set of language storage ModuleId's that this module declares as friends
    pub fn get_friends(&self) -> Vec<language_storage::ModuleId> {
        self.data.module.immediate_friends()
    }

    /// Returns the set of modules that use this one.
    pub fn get_using_modules(&self) -> BTreeSet<ModuleId> {
        self.env
            .get_modules()
            .filter_map(|module_env| {
                if module_env.get_used_modules().contains(&self.data.id) {
                    Some(module_env.data.id)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Returns the set of modules this one uses.
    pub fn get_used_modules(&self) -> BTreeSet<ModuleId> {
        if let Some(usage) = self.data.used_modules.borrow().get(&false) {
            return usage.clone();
        }
        // Determine modules used in bytecode from the compiled module.
        let usage: BTreeSet<ModuleId> = self
            .get_dependencies()
            .into_iter()
            .map(|storage_id| self.env.to_module_name(&storage_id))
            .filter_map(|name| self.env.find_module(&name))
            .map(|env| env.get_id())
            .filter(|id| *id != self.get_id())
            .collect();
        self.data
            .used_modules
            .borrow_mut()
            .insert(false, usage.clone());
        usage
    }

    /// Returns the set of modules this one declares as friends.
    pub fn get_friend_modules(&self) -> BTreeSet<ModuleId> {
        self.data
            .friend_modules
            .borrow_mut()
            .get_or_insert_with(|| {
                // Determine modules used in bytecode from the compiled module.
                self.get_friends()
                    .into_iter()
                    .map(|storage_id| self.env.to_module_name(&storage_id))
                    .filter_map(|name| self.env.find_module(&name))
                    .map(|env| env.get_id())
                    .collect()
            })
            .clone()
    }

    /// Returns true if the given module is a transitive dependency of this one. The
    /// transitive dependency set contains this module and all directly or indirectly used
    /// modules (without spec usage).
    pub fn is_transitive_dependency(&self, module_id: ModuleId) -> bool {
        if self.get_id() == module_id {
            true
        } else {
            for dep in self.get_used_modules() {
                if self.env.get_module(dep).is_transitive_dependency(module_id) {
                    return true;
                }
            }
            false
        }
    }

    /// Shortcut for accessing the symbol pool.
    pub fn symbol_pool(&self) -> &SymbolPool {
        &self.env.symbol_pool
    }

    /// Gets the underlying bytecode module.
    pub fn get_verified_module(&'env self) -> &'env CompiledModule {
        &self.data.module
    }

    /// Gets a `NamedConstantEnv` in this module by name
    pub fn find_named_constant(&'env self, name: Symbol) -> Option<NamedConstantEnv<'env>> {
        let id = NamedConstantId(name);
        self.data
            .named_constants
            .get(&id)
            .map(|data| NamedConstantEnv {
                module_env: self.clone(),
                data,
            })
    }

    /// Gets a `NamedConstantEnv` in this module by the constant's id
    pub fn get_named_constant(&'env self, id: NamedConstantId) -> NamedConstantEnv<'env> {
        self.clone().into_named_constant(id)
    }

    /// Gets a `NamedConstantEnv` by id
    pub fn into_named_constant(self, id: NamedConstantId) -> NamedConstantEnv<'env> {
        let data = self
            .data
            .named_constants
            .get(&id)
            .expect("NamedConstantId undefined");
        NamedConstantEnv {
            module_env: self,
            data,
        }
    }

    /// Gets the number of named constants in this module.
    pub fn get_named_constant_count(&self) -> usize {
        self.data.named_constants.len()
    }

    /// Returns iterator over `NamedConstantEnv`s in this module.
    pub fn get_named_constants(&'env self) -> impl Iterator<Item = NamedConstantEnv<'env>> {
        self.clone().into_named_constants()
    }

    /// Returns an iterator over `NamedConstantEnv`s in this module.
    pub fn into_named_constants(self) -> impl Iterator<Item = NamedConstantEnv<'env>> {
        self.data
            .named_constants
            .values()
            .map(move |data| NamedConstantEnv {
                module_env: self.clone(),
                data,
            })
    }

    /// Gets a FunctionEnv in this module by name.
    pub fn find_function(&self, name: Symbol) -> Option<FunctionEnv<'env>> {
        let id = FunId(name);
        self.data
            .function_data
            .get(&id)
            .map(move |data| FunctionEnv {
                module_env: self.clone(),
                data,
            })
    }

    /// Gets a FunctionEnv by id.
    pub fn get_function(&'env self, id: FunId) -> FunctionEnv<'env> {
        self.clone().into_function(id)
    }

    /// Gets a FunctionEnv by id.
    pub fn into_function(self, id: FunId) -> FunctionEnv<'env> {
        let data = self.data.function_data.get(&id).expect("FunId undefined");
        FunctionEnv {
            module_env: self,
            data,
        }
    }

    /// Gets the number of functions in this module.
    pub fn get_function_count(&self) -> usize {
        self.data.function_data.len()
    }

    /// Returns iterator over FunctionEnvs in this module.
    pub fn get_functions(&'env self) -> impl Iterator<Item = FunctionEnv<'env>> {
        self.clone().into_functions()
    }

    /// Returns iterator over FunctionEnvs in this module.
    pub fn into_functions(self) -> impl Iterator<Item = FunctionEnv<'env>> {
        self.data
            .function_data
            .values()
            .map(move |data| FunctionEnv {
                module_env: self.clone(),
                data,
            })
    }

    /// Gets FunctionEnv for a function used in this module, via the FunctionHandleIndex. The
    /// returned function might be from this or another module.
    pub fn get_used_function(&self, idx: FunctionHandleIndex) -> FunctionEnv<'_> {
        let module = &self.data.module;
        let fhandle = module.function_handle_at(idx);
        let fname = module.identifier_at(fhandle.name).as_str();
        let declaring_module_handle = module.module_handle_at(fhandle.module);
        let declaring_module = module.module_id_for_handle(declaring_module_handle);
        let module_env = self
            .env
            .find_module(&self.env.to_module_name(&declaring_module))
            .expect("unexpected reference to module not found in global env");
        module_env.into_function(FunId::new(self.env.symbol_pool.make(fname)))
    }

    /// Gets the function id from a definition index.
    pub fn try_get_function_id(&self, idx: FunctionDefinitionIndex) -> Option<FunId> {
        self.data.function_idx_to_id.get(&idx).cloned()
    }

    /// Gets the function definition index for the given function id. This is always defined.
    pub fn get_function_def_idx(&self, fun_id: FunId) -> FunctionDefinitionIndex {
        self.data
            .function_data
            .get(&fun_id)
            .expect("function id defined")
            .def_idx
    }

    /// Gets a StructEnv in this module by name.
    pub fn find_struct(&self, name: Symbol) -> Option<StructEnv<'_>> {
        let id = DatatypeId(name);
        self.data.struct_data.get(&id).map(|data| StructEnv {
            module_env: self.clone(),
            data,
        })
    }

    /// Gets a StructEnv in this module by identifier
    pub fn find_struct_by_identifier(&self, identifier: Identifier) -> Option<DatatypeId> {
        let some_id = Some(identifier);
        for data in self.data.struct_data.values() {
            let senv = StructEnv {
                module_env: self.clone(),
                data,
            };
            if senv.get_identifier() == some_id {
                return Some(senv.get_id());
            }
        }
        None
    }

    /// Gets the struct id from a definition index which must be valid for this environment.
    pub fn get_struct_id(&self, idx: StructDefinitionIndex) -> DatatypeId {
        *self
            .data
            .struct_idx_to_id
            .get(&idx)
            .unwrap_or_else(|| panic!("undefined struct definition index {:?}", idx))
    }

    /// Gets a StructEnv by id.
    pub fn get_struct(&self, id: DatatypeId) -> StructEnv<'_> {
        let data = self.data.struct_data.get(&id).expect("StructId undefined");
        StructEnv {
            module_env: self.clone(),
            data,
        }
    }

    pub fn get_struct_by_def_idx(&self, idx: StructDefinitionIndex) -> StructEnv<'_> {
        self.get_struct(self.get_struct_id(idx))
    }

    /// Gets a StructEnv by id, consuming this module env.
    pub fn into_struct(self, id: DatatypeId) -> StructEnv<'env> {
        let data = self.data.struct_data.get(&id).expect("StructId undefined");
        StructEnv {
            module_env: self,
            data,
        }
    }

    /// Gets the number of structs in this module.
    pub fn get_struct_count(&self) -> usize {
        self.data.struct_data.len()
    }

    /// Returns an iterator over structs in this module.
    pub fn get_structs(&'env self) -> impl Iterator<Item = StructEnv<'env>> {
        self.clone().into_structs()
    }

    /// Gets an EnumEnv in this module by name.
    pub fn find_enum(&self, name: Symbol) -> Option<EnumEnv<'_>> {
        let id = DatatypeId(name);
        self.data.enum_data.get(&id).map(|data| EnumEnv {
            module_env: self.clone(),
            data,
        })
    }

    /// Gets an EnumEnv in this module by identifier
    pub fn find_enum_by_identifier(&self, identifier: Identifier) -> Option<DatatypeId> {
        let some_id = Some(identifier);
        for data in self.data.enum_data.values() {
            let eenv = EnumEnv {
                module_env: self.clone(),
                data,
            };
            if eenv.get_identifier() == some_id {
                return Some(eenv.get_id());
            }
        }
        None
    }

    /// Gets the enum id from a definition index which must be valid for this environment.
    pub fn get_enum_id(&self, idx: EnumDefinitionIndex) -> DatatypeId {
        *self
            .data
            .enum_idx_to_id
            .get(&idx)
            .unwrap_or_else(|| panic!("undefined enum definition index {:?}", idx))
    }

    /// Gets an EnumEnv by id.
    pub fn get_enum(&self, id: DatatypeId) -> EnumEnv<'_> {
        let data = self.data.enum_data.get(&id).expect("EnumId undefined");
        EnumEnv {
            module_env: self.clone(),
            data,
        }
    }

    pub fn get_enum_by_def_idx(&self, idx: EnumDefinitionIndex) -> EnumEnv<'_> {
        self.get_enum(self.get_enum_id(idx))
    }

    /// Gets an EnumEnv by id, consuming this module env.
    pub fn into_enum(self, id: DatatypeId) -> EnumEnv<'env> {
        let data = self.data.enum_data.get(&id).expect("EnumId undefined");
        EnumEnv {
            module_env: self,
            data,
        }
    }

    /// Gets the number of enums in this module.
    pub fn get_enum_count(&self) -> usize {
        self.data.enum_data.len()
    }

    /// Returns an iterator over structs in this module.
    pub fn get_enums(&'env self) -> impl Iterator<Item = EnumEnv<'env>> {
        self.clone().into_enums()
    }

    /// Returns an iterator over all object types declared by this module
    pub fn get_objects(&'env self) -> impl Iterator<Item = StructEnv<'env>> {
        self.clone()
            .into_structs()
            .filter(|s| s.get_abilities().has_key())
    }

    /// Returns the object types that are shared by code in this module
    /// If `transitive` is false, only return objects directly shared by functions declared in this module
    /// If `transitive` is true, return objects shared by both functions declared in this module and by transitive callees
    /// Note that this can include both types declared inside this module (common case) and types declared outside
    /// Note that objects with `store` can be shared by modules that depend on this one (e.g., by returning the object and subsequently calling `public_share_object`)
    pub fn get_shared_objects(&'env self, transitive: bool) -> BTreeSet<Type> {
        let mut shared = BTreeSet::new();
        for f in self.get_functions() {
            shared.extend(f.get_shared_objects(transitive));
        }
        shared
    }

    /// Returns the object types that are frozen by this module
    /// If `transitive` is false, only return objects directly transferred by functions declared in this module
    /// If `transitive` is true, return objects transferred by both functions declared in this module and by transitive callees
    /// Note that this function can return both types declared inside this module (common case) and types declared outside
    /// Note that objects with `store` can be transferred by modules that depend on this one (e.g., by returning the object and subsequently calling `public_transfer`),
    /// or transferred by a command in a programmable transaction block
    pub fn get_transferred_objects(&'env self, transitive: bool) -> BTreeSet<Type> {
        let mut transferred = BTreeSet::new();
        for f in self.get_functions() {
            transferred.extend(f.get_transferred_objects(transitive))
        }
        transferred
    }

    /// Returns the object types that are frozen by this module
    /// If `transitive` is false, only return objects directly frozen by functions declared in this module
    /// If `transitive` is true, return objects frozen by both functions declared in this module and by transitive callees
    /// Note that this function can return both types declared inside this module (common case) and types declared outside
    /// Note that objects with `store` can be frozen by modules that depend on this one (e.g., by returning the object and subsequently calling `public_freeze`)
    pub fn get_frozen_objects(&'env self, transitive: bool) -> BTreeSet<Type> {
        let mut frozen = BTreeSet::new();
        for f in self.get_functions() {
            frozen.extend(f.get_frozen_objects(transitive))
        }
        frozen
    }

    /// Returns the event types that are emitted by this module
    /// If `transitive` is false, only return events directly emitted by functions declared in this module
    /// If `transitive` is true, return events emitted by both functions declared in this module and by transitive callees
    /// Note that this function can return both event types declared inside this module (common case) and event types declared outside
    pub fn get_events(&'env self, transitive: bool) -> BTreeSet<Type> {
        let mut frozen = BTreeSet::new();
        for f in self.get_functions() {
            frozen.extend(f.get_frozen_objects(transitive))
        }
        frozen
    }

    /// Returns the objects types that are returned by externally callable (`public`, `entry`, and `friend`) functions in this module
    /// Returned objects with `store` can be transferred, shared, frozen, or wrapped by a different module
    /// Note that this function returns object types both with and without `store`
    pub fn get_externally_returned_objects(&'env self) -> BTreeSet<Type> {
        let mut returned = BTreeSet::new();
        for f in self.get_functions() {
            if !f.is_exposed() {
                continue;
            }
            // Objects returned by a public function can be transferred, shared, frozen, or wrapped
            // by a different module or (in the case of transfer) by a command in a programmable transaction block.
            for f in f.get_return_types() {
                if let Type::Datatype(mid, sid, _) = f {
                    let struct_env = self.env.get_module(mid).into_struct(sid);
                    if struct_env.get_abilities().has_key() {
                        returned.insert(f);
                    }
                }
            }
        }
        returned
    }

    /// Returns iterator over structs in this module.
    pub fn into_structs(self) -> impl Iterator<Item = StructEnv<'env>> {
        self.data.struct_data.values().map(move |data| StructEnv {
            module_env: self.clone(),
            data,
        })
    }

    /// Returns iterator over enums in this module.
    pub fn into_enums(self) -> impl Iterator<Item = EnumEnv<'env>> {
        self.data.enum_data.values().map(move |data| EnumEnv {
            module_env: self.clone(),
            data,
        })
    }

    /// Globalizes a signature local to this module.
    pub fn globalize_signature(&self, sig: &SignatureToken) -> Type {
        match sig {
            SignatureToken::Bool => Type::Primitive(PrimitiveType::Bool),
            SignatureToken::U8 => Type::Primitive(PrimitiveType::U8),
            SignatureToken::U16 => Type::Primitive(PrimitiveType::U16),
            SignatureToken::U32 => Type::Primitive(PrimitiveType::U32),
            SignatureToken::U64 => Type::Primitive(PrimitiveType::U64),
            SignatureToken::U128 => Type::Primitive(PrimitiveType::U128),
            SignatureToken::U256 => Type::Primitive(PrimitiveType::U256),
            SignatureToken::Address => Type::Primitive(PrimitiveType::Address),
            SignatureToken::Signer => Type::Primitive(PrimitiveType::Signer),
            SignatureToken::Reference(t) => {
                Type::Reference(false, Box::new(self.globalize_signature(t)))
            }
            SignatureToken::MutableReference(t) => {
                Type::Reference(true, Box::new(self.globalize_signature(t)))
            }
            SignatureToken::TypeParameter(index) => Type::TypeParameter(*index),
            SignatureToken::Vector(bt) => Type::Vector(Box::new(self.globalize_signature(bt))),
            SignatureToken::Datatype(handle_idx) => {
                let module = &self.data.module;
                let shandle = module.datatype_handle_at(*handle_idx);
                let sname = module.identifier_at(shandle.name).as_str();
                let declaring_module_handle = module.module_handle_at(shandle.module);
                let declaring_module = module.module_id_for_handle(declaring_module_handle);
                let declaring_module_env = self
                    .env
                    .find_module(&self.env.to_module_name(&declaring_module))
                    .expect("undefined module");
                let name = self.env.symbol_pool.make(sname);
                let datatype_id = declaring_module_env
                    .find_struct(name)
                    .map(|env| env.get_id())
                    .or_else(|| declaring_module_env.find_enum(name).map(|env| env.get_id()))
                    .expect("undefined datatype");
                Type::Datatype(declaring_module_env.data.id, datatype_id, vec![])
            }
            SignatureToken::DatatypeInstantiation(inst) => {
                let (handle_idx, args) = &**inst;
                let module = &self.data.module;
                let shandle = module.datatype_handle_at(*handle_idx);
                let sname = module.identifier_at(shandle.name).as_str();
                let declaring_module_handle = module.module_handle_at(shandle.module);
                let declaring_module = module.module_id_for_handle(declaring_module_handle);
                let declaring_module_env = self
                    .env
                    .find_module(&self.env.to_module_name(&declaring_module))
                    .expect("undefined module");
                let name = self.env.symbol_pool.make(sname);
                let datatype_id = declaring_module_env
                    .find_struct(name)
                    .map(|env| env.get_id())
                    .or_else(|| declaring_module_env.find_enum(name).map(|env| env.get_id()))
                    .expect("undefined datatype");
                Type::Datatype(
                    declaring_module_env.data.id,
                    datatype_id,
                    self.globalize_signatures(args),
                )
            }
        }
    }

    /// Globalizes a list of signatures.
    pub fn globalize_signatures(&self, sigs: &[SignatureToken]) -> Vec<Type> {
        sigs.iter()
            .map(|s| self.globalize_signature(s))
            .collect_vec()
    }

    /// Gets a list of type actuals associated with the index in the bytecode.
    pub fn get_type_actuals(&self, idx: Option<SignatureIndex>) -> Vec<Type> {
        match idx {
            Some(idx) => {
                let actuals = &self.data.module.signature_at(idx).0;
                self.globalize_signatures(actuals)
            }
            None => vec![],
        }
    }

    /// Retrieve a constant from the pool
    pub fn get_constant(&self, idx: ConstantPoolIndex) -> &VMConstant {
        &self.data.module.constant_pool()[idx.0 as usize]
    }

    /// Converts a constant to the specified type. The type must correspond to the expected
    /// cannonical representation as defined in `move_core_types::values`
    pub fn get_constant_value(&self, constant: &VMConstant) -> MoveValue {
        VMConstant::deserialize_constant(constant).unwrap()
    }

    /// Return the `AccountAdress` of this module
    pub fn self_address(&self) -> &AccountAddress {
        self.data.module.address()
    }

    /// Retrieve an address identifier from the pool
    pub fn get_address_identifier(&self, idx: AddressIdentifierIndex) -> BigUint {
        let addr = &self.data.module.address_identifiers()[idx.0 as usize];
        crate::addr_to_big_uint(addr)
    }

    /// Disassemble the module bytecode
    pub fn disassemble(&self) -> String {
        let disas = Disassembler::new(
            SourceMapping::new(self.data.source_map.clone(), self.get_verified_module()),
            DisassemblerOptions {
                only_externally_visible: false,
                print_code: true,
                print_basic_blocks: true,
                print_locals: true,
                max_output_size: None,
            },
        );
        disas
            .disassemble()
            .expect("Failed to disassemble a verified module")
    }

    fn match_module_name(&self, module_name: &str) -> bool {
        self.get_name()
            .name()
            .display(self.env.symbol_pool())
            .to_string()
            == module_name
    }

    fn is_module_in_std(&self, module_name: &str) -> bool {
        let addr = self.get_name().addr();
        *addr == self.env.get_stdlib_address() && self.match_module_name(module_name)
    }

    fn is_module_in_ext(&self, module_name: &str) -> bool {
        let addr = self.get_name().addr();
        *addr == self.env.get_extlib_address() && self.match_module_name(module_name)
    }

    pub fn is_std_vector(&self) -> bool {
        self.is_module_in_std("vector")
    }

    pub fn is_table(&self) -> bool {
        self.is_module_in_std("table")
            || self.is_module_in_std("table_with_length")
            || self.is_module_in_ext("table")
            || self.is_module_in_ext("table_with_length")
    }
}

// =================================================================================================
/// # Enum Environment

#[derive(Debug)]
pub struct EnumData {
    /// The name of this enum.
    name: Symbol,

    /// The location of this enum.
    loc: Loc,

    /// Attributes attached to this enum.
    attributes: Vec<Attribute>,

    /// The definition index of this enum in its module.
    def_idx: EnumDefinitionIndex,

    /// The handle index of this enum in its module.
    handle_idx: DatatypeHandleIndex,

    /// Variant definitions
    variant_data: BTreeMap<VariantId, VariantData>,
}

#[derive(Debug, Clone)]
pub struct EnumEnv<'env> {
    /// Reference to enclosing module.
    pub module_env: ModuleEnv<'env>,

    /// Reference to the enum data.
    data: &'env EnumData,
}

impl<'env> EnumEnv<'env> {
    /// Returns the name of this enum.
    pub fn get_name(&self) -> Symbol {
        self.data.name
    }

    /// Gets full name as string.
    pub fn get_full_name_str(&self) -> String {
        format!(
            "{}::{}",
            self.module_env.get_name().display(self.symbol_pool()),
            self.get_name().display(self.symbol_pool())
        )
    }

    /// Gets full name with module address as string.
    pub fn get_full_name_with_address(&self) -> String {
        format!(
            "{}::{}",
            self.module_env.get_full_name_str(),
            self.get_name().display(self.symbol_pool())
        )
    }

    /// Returns the VM identifier for thisenum
    pub fn get_identifier(&self) -> Option<Identifier> {
        let handle_idx = self.data.handle_idx;
        let handle = self.module_env.data.module.datatype_handle_at(handle_idx);
        Some(
            self.module_env
                .data
                .module
                .identifier_at(handle.name)
                .to_owned(),
        )
    }

    /// Shortcut for accessing the symbol pool.
    pub fn symbol_pool(&self) -> &SymbolPool {
        self.module_env.symbol_pool()
    }

    /// Returns the location of this enum.
    pub fn get_loc(&self) -> Loc {
        self.data.loc.clone()
    }

    /// Returns the attributes of this enum.
    pub fn get_attributes(&self) -> &[Attribute] {
        &self.data.attributes
    }

    /// Gets the id associated with this enum.
    pub fn get_id(&self) -> DatatypeId {
        DatatypeId(self.data.name)
    }

    /// Gets the qualified id of this enum.
    pub fn get_qualified_id(&self) -> QualifiedId<DatatypeId> {
        self.module_env.get_id().qualified(self.get_id())
    }

    /// Get the abilities of this struct.
    pub fn get_abilities(&self) -> AbilitySet {
        let def = self.module_env.data.module.enum_def_at(self.data.def_idx);
        let handle = self
            .module_env
            .data
            .module
            .datatype_handle_at(def.enum_handle);
        handle.abilities
    }

    /// Determines whether memory-related operations needs to be declared for this struct.
    pub fn has_memory(&self) -> bool {
        self.get_abilities().has_key()
    }

    /// Get an iterator for the fields, ordered by offset.
    pub fn get_variants(&'env self) -> impl Iterator<Item = VariantEnv<'env>> {
        self.data
            .variant_data
            .values()
            .sorted_by_key(|data| data.tag)
            .map(move |data| VariantEnv {
                enum_env: self.clone(),
                data,
            })
    }

    /// Return the number of variants in the enum.
    pub fn get_variant_count(&self) -> usize {
        self.data.variant_data.len()
    }

    /// Gets a variant by its id.
    pub fn get_variant(&'env self, id: VariantId) -> VariantEnv<'env> {
        let data = self
            .data
            .variant_data
            .get(&id)
            .expect("VariantId undefined");
        VariantEnv {
            enum_env: self.clone(),
            data,
        }
    }

    /// Find a variann by its name.
    pub fn find_variant(&'env self, name: Symbol) -> Option<VariantEnv<'env>> {
        let id = VariantId(name);
        self.data.variant_data.get(&id).map(|data| VariantEnv {
            enum_env: self.clone(),
            data,
        })
    }

    /// Gets a variant by its tag.
    pub fn get_variant_by_tag(&'env self, tag: usize) -> VariantEnv<'env> {
        for data in self.data.variant_data.values() {
            if data.tag == tag {
                return VariantEnv {
                    enum_env: self.clone(),
                    data,
                };
            }
        }
        unreachable!("invalid variant lookup")
    }

    /// Whether the type parameter at position `idx` is declared as phantom.
    pub fn is_phantom_parameter(&self, idx: usize) -> bool {
        let def_idx = self.data.def_idx;

        let def = self.module_env.data.module.enum_def_at(def_idx);
        self.module_env
            .data
            .module
            .datatype_handle_at(def.enum_handle)
            .type_parameters[idx]
            .is_phantom
    }

    /// Returns the type parameters associated with this enum.
    pub fn get_type_parameters(&self) -> Vec<TypeParameter> {
        // TODO: we currently do not know the original names of those formals, so we generate them.
        let pool = &self.module_env.env.symbol_pool;
        let def_idx = self.data.def_idx;
        let module = &self.module_env.data.module;
        let edef = module.enum_def_at(def_idx);
        let ehandle = module.datatype_handle_at(edef.enum_handle);
        ehandle
            .type_parameters
            .iter()
            .enumerate()
            .map(|(i, k)| {
                TypeParameter(
                    pool.make(&format!("$tv{}", i)),
                    AbilityConstraint(k.constraints),
                )
            })
            .collect_vec()
    }

    /// Returns the type parameters associated with this enum, with actual names.
    pub fn get_named_type_parameters(&self) -> Vec<TypeParameter> {
        let def_idx = self.data.def_idx;
        let module = &self.module_env.data.module;
        let edef = module.enum_def_at(def_idx);
        let ehandle = module.datatype_handle_at(edef.enum_handle);
        ehandle
            .type_parameters
            .iter()
            .enumerate()
            .map(|(i, k)| {
                let name = self
                    .module_env
                    .data
                    .source_map
                    .get_enum_source_map(def_idx)
                    .ok()
                    .and_then(|smap| smap.type_parameters.get(i))
                    .map(|(s, _)| s.clone())
                    .unwrap_or_else(|| format!("unknown#{}", i));
                TypeParameter(
                    self.module_env.env.symbol_pool.make(&name),
                    AbilityConstraint(k.constraints),
                )
            })
            .collect_vec()
    }
}

// =================================================================================================
/// # Variant Environment

#[derive(Debug)]
pub struct VariantData {
    /// The name of this variant.
    name: Symbol,

    /// The location of this variant.
    loc: Loc,

    tag: usize,

    /// Field definitions.
    field_data: BTreeMap<FieldId, FieldData>,
}

#[derive(Debug, Clone)]
pub struct VariantEnv<'env> {
    /// Reference to enclosing module.
    pub enum_env: EnumEnv<'env>,

    /// Reference to the variant data.
    data: &'env VariantData,
}

impl<'env> VariantEnv<'env> {
    /// Returns the name of this variant.
    pub fn get_name(&self) -> Symbol {
        self.data.name
    }

    /// Gets full name as string.
    pub fn get_full_name_str(&self) -> String {
        format!(
            "{}::{}::{}",
            self.enum_env
                .module_env
                .get_name()
                .display(self.symbol_pool()),
            self.enum_env.get_name().display(self.symbol_pool()),
            self.get_name().display(self.symbol_pool())
        )
    }

    /// Gets full name with module address as string.
    pub fn get_full_name_with_address(&self) -> String {
        format!(
            "{}::{}",
            self.enum_env.get_full_name_str(),
            self.get_name().display(self.symbol_pool())
        )
    }

    /// Gets the tag associated with this variant.
    pub fn get_tag(&self) -> usize {
        self.data.tag
    }

    /// Returns the VM identifier for this variant
    pub fn get_identifier(&self) -> Option<Identifier> {
        let enum_def = self
            .enum_env
            .module_env
            .data
            .module
            .enum_def_at(self.enum_env.data.def_idx);
        let variant_def = &enum_def.variants[self.data.tag];
        Some(
            self.enum_env
                .module_env
                .data
                .module
                .identifier_at(variant_def.variant_name)
                .to_owned(),
        )
    }

    /// Shortcut for accessing the symbol pool.
    pub fn symbol_pool(&self) -> &SymbolPool {
        self.enum_env.symbol_pool()
    }

    /// Returns the location of this variant.
    pub fn get_loc(&self) -> Loc {
        self.data.loc.clone()
    }

    /// Gets the id associated with this variant.
    pub fn get_id(&self) -> VariantId {
        VariantId(self.data.name)
    }

    /// Get an iterator for the fields, ordered by offset.
    pub fn get_fields(&'env self) -> impl Iterator<Item = FieldEnv<'env>> {
        self.data
            .field_data
            .values()
            .sorted_by_key(|data| data.offset)
            .map(move |data| FieldEnv {
                parent_env: EnclosingEnv::Variant(self.clone()),
                data,
            })
    }

    /// Return the number of fields in the struct.
    pub fn get_field_count(&self) -> usize {
        self.data.field_data.len()
    }

    /// Gets a field by its id.
    pub fn get_field(&'env self, id: FieldId) -> FieldEnv<'env> {
        let data = self.data.field_data.get(&id).expect("FieldId undefined");
        FieldEnv {
            parent_env: EnclosingEnv::Variant(self.clone()),
            data,
        }
    }

    /// Find a field by its name.
    pub fn find_field(&'env self, name: Symbol) -> Option<FieldEnv<'env>> {
        let id = FieldId(name);
        self.data.field_data.get(&id).map(|data| FieldEnv {
            parent_env: EnclosingEnv::Variant(self.clone()),
            data,
        })
    }

    /// Gets a field by its offset.
    pub fn get_field_by_offset(&'env self, offset: usize) -> FieldEnv<'env> {
        for data in self.data.field_data.values() {
            if data.offset == offset {
                return FieldEnv {
                    parent_env: EnclosingEnv::Variant(self.clone()),
                    data,
                };
            }
        }
        unreachable!("invalid field lookup")
    }
}

// =================================================================================================
/// # Struct Environment

#[derive(Debug)]
pub struct StructData {
    /// The name of this struct.
    name: Symbol,

    /// The location of this struct.
    loc: Loc,

    /// Attributes attached to this structure.
    attributes: Vec<Attribute>,

    /// List of function argument names. Not in bytecode but obtained from AST.
    /// Information about this struct.
    info: StructInfo,

    /// Field definitions.
    field_data: BTreeMap<FieldId, FieldData>,
}

#[derive(Debug)]
enum StructInfo {
    /// Struct is declared in Move and info found in VM format.
    Declared {
        /// The definition index of this struct in its module.
        def_idx: StructDefinitionIndex,

        /// The handle index of this struct in its module.
        handle_idx: DatatypeHandleIndex,
    },
}

#[derive(Debug, Clone)]
pub struct StructEnv<'env> {
    /// Reference to enclosing module.
    pub module_env: ModuleEnv<'env>,

    /// Reference to the struct data.
    data: &'env StructData,
}

impl<'env> StructEnv<'env> {
    /// Returns the name of this struct.
    pub fn get_name(&self) -> Symbol {
        self.data.name
    }

    /// Gets full name as string.
    pub fn get_full_name_str(&self) -> String {
        format!(
            "{}::{}",
            self.module_env.get_name().display(self.symbol_pool()),
            self.get_name().display(self.symbol_pool())
        )
    }

    /// Gets full name with module address as string.
    pub fn get_full_name_with_address(&self) -> String {
        format!(
            "{}::{}",
            self.module_env.get_full_name_str(),
            self.get_name().display(self.symbol_pool())
        )
    }

    /// Returns the VM identifier for this struct
    pub fn get_identifier(&self) -> Option<Identifier> {
        match &self.data.info {
            StructInfo::Declared { handle_idx, .. } => {
                let handle = self.module_env.data.module.datatype_handle_at(*handle_idx);
                Some(
                    self.module_env
                        .data
                        .module
                        .identifier_at(handle.name)
                        .to_owned(),
                )
            }
        }
    }

    /// Shortcut for accessing the symbol pool.
    pub fn symbol_pool(&self) -> &SymbolPool {
        self.module_env.symbol_pool()
    }

    /// Returns the location of this struct.
    pub fn get_loc(&self) -> Loc {
        self.data.loc.clone()
    }

    /// Returns the attributes of this struct.
    pub fn get_attributes(&self) -> &[Attribute] {
        &self.data.attributes
    }

    /// Gets the id associated with this struct.
    pub fn get_id(&self) -> DatatypeId {
        DatatypeId(self.data.name)
    }

    /// Gets the qualified id of this struct.
    pub fn get_qualified_id(&self) -> QualifiedId<DatatypeId> {
        self.module_env.get_id().qualified(self.get_id())
    }

    /// Determines whether this struct is native.
    pub fn is_native(&self) -> bool {
        match &self.data.info {
            StructInfo::Declared { def_idx, .. } => {
                let def = self.module_env.data.module.struct_def_at(*def_idx);
                def.field_information == StructFieldInformation::Native
            }
        }
    }

    /// Get the abilities of this struct.
    pub fn get_abilities(&self) -> AbilitySet {
        match &self.data.info {
            StructInfo::Declared { def_idx, .. } => {
                let def = self.module_env.data.module.struct_def_at(*def_idx);
                let handle = self
                    .module_env
                    .data
                    .module
                    .datatype_handle_at(def.struct_handle);
                handle.abilities
            }
        }
    }

    /// Determines whether memory-related operations needs to be declared for this struct.
    pub fn has_memory(&self) -> bool {
        self.get_abilities().has_key()
    }

    /// Get an iterator for the fields, ordered by offset.
    pub fn get_fields(&'env self) -> impl Iterator<Item = FieldEnv<'env>> {
        self.data
            .field_data
            .values()
            .sorted_by_key(|data| data.offset)
            .map(move |data| FieldEnv {
                parent_env: EnclosingEnv::Struct(self.clone()),
                data,
            })
    }

    /// Return the number of fields in the struct.
    pub fn get_field_count(&self) -> usize {
        self.data.field_data.len()
    }

    /// Gets a field by its id.
    pub fn get_field(&'env self, id: FieldId) -> FieldEnv<'env> {
        let data = self.data.field_data.get(&id).expect("FieldId undefined");
        FieldEnv {
            parent_env: EnclosingEnv::Struct(self.clone()),
            data,
        }
    }

    /// Find a field by its name.
    pub fn find_field(&'env self, name: Symbol) -> Option<FieldEnv<'env>> {
        let id = FieldId(name);
        self.data.field_data.get(&id).map(|data| FieldEnv {
            parent_env: EnclosingEnv::Struct(self.clone()),
            data,
        })
    }

    /// Gets a field by its offset.
    pub fn get_field_by_offset(&'env self, offset: usize) -> FieldEnv<'env> {
        for data in self.data.field_data.values() {
            if data.offset == offset {
                return FieldEnv {
                    parent_env: EnclosingEnv::Struct(self.clone()),
                    data,
                };
            }
        }
        unreachable!("invalid field lookup")
    }

    /// Whether the type parameter at position `idx` is declared as phantom.
    pub fn is_phantom_parameter(&self, idx: usize) -> bool {
        match &self.data.info {
            StructInfo::Declared { def_idx, .. } => {
                let def = self.module_env.data.module.struct_def_at(*def_idx);
                self.module_env
                    .data
                    .module
                    .datatype_handle_at(def.struct_handle)
                    .type_parameters[idx]
                    .is_phantom
            }
        }
    }

    /// Returns the type parameters associated with this struct.
    pub fn get_type_parameters(&self) -> Vec<TypeParameter> {
        // TODO: we currently do not know the original names of those formals, so we generate them.
        let pool = &self.module_env.env.symbol_pool;
        match &self.data.info {
            StructInfo::Declared { def_idx, .. } => {
                let module = &self.module_env.data.module;
                let sdef = module.struct_def_at(*def_idx);
                let shandle = module.datatype_handle_at(sdef.struct_handle);
                shandle
                    .type_parameters
                    .iter()
                    .enumerate()
                    .map(|(i, k)| {
                        TypeParameter(
                            pool.make(&format!("$tv{}", i)),
                            AbilityConstraint(k.constraints),
                        )
                    })
                    .collect_vec()
            }
        }
    }

    /// Returns the type parameters associated with this struct, with actual names.
    pub fn get_named_type_parameters(&self) -> Vec<TypeParameter> {
        match &self.data.info {
            StructInfo::Declared { def_idx, .. } => {
                let module = &self.module_env.data.module;
                let sdef = module.struct_def_at(*def_idx);
                let shandle = module.datatype_handle_at(sdef.struct_handle);
                shandle
                    .type_parameters
                    .iter()
                    .enumerate()
                    .map(|(i, k)| {
                        let name = self
                            .module_env
                            .data
                            .source_map
                            .get_struct_source_map(*def_idx)
                            .ok()
                            .and_then(|smap| smap.type_parameters.get(i))
                            .map(|(s, _)| s.clone())
                            .unwrap_or_else(|| format!("unknown#{}", i));
                        TypeParameter(
                            self.module_env.env.symbol_pool.make(&name),
                            AbilityConstraint(k.constraints),
                        )
                    })
                    .collect_vec()
            }
        }
    }
}

// =================================================================================================
/// # Field Environment

#[derive(Debug)]
pub struct FieldData {
    /// The name of this field.
    name: Symbol,

    /// The offset of this field.
    offset: usize,

    /// More information about this field
    info: FieldInfo,
}

#[derive(Debug)]
enum FieldInfo {
    /// The field is declared in Move.
    DeclaredStruct {
        /// The struct definition index of this field in its VM module.
        def_idx: StructDefinitionIndex,
    },
    DeclaredEnum {
        /// The enum definition index of this field in its VM module.
        def_idx: EnumDefinitionIndex,
    },
}

#[derive(Debug)]
pub enum EnclosingEnv<'env> {
    Struct(StructEnv<'env>),
    Variant(VariantEnv<'env>),
}

impl<'env> EnclosingEnv<'env> {
    pub fn module_env(&self) -> &ModuleEnv<'env> {
        match self {
            EnclosingEnv::Struct(s) => &s.module_env,
            EnclosingEnv::Variant(v) => &v.enum_env.module_env,
        }
    }
}

#[derive(Debug)]
pub struct FieldEnv<'env> {
    /// Reference to enclosing env.
    pub parent_env: EnclosingEnv<'env>,

    /// Reference to the field data.
    data: &'env FieldData,
}

impl<'env> FieldEnv<'env> {
    /// Gets the name of this field.
    pub fn get_name(&self) -> Symbol {
        self.data.name
    }

    /// Gets the id of this field.
    pub fn get_id(&self) -> FieldId {
        FieldId(self.data.name)
    }

    /// Returns the VM identifier for this field
    pub fn get_identifier(&'env self) -> Option<Identifier> {
        match &self.data.info {
            FieldInfo::DeclaredStruct { def_idx } => {
                let module = &self.parent_env.module_env().data.module;
                let def = module.struct_def_at(*def_idx);
                let offset = self.data.offset;
                let field = def.field(offset).expect("Bad field offset");
                Some(module.identifier_at(field.name).to_owned())
            }
            FieldInfo::DeclaredEnum { def_idx } => {
                let EnclosingEnv::Variant(v) = &self.parent_env else {
                    unreachable!()
                };
                let m = &v.enum_env.module_env.data.module;
                let enum_def = m.enum_def_at(*def_idx);
                let variant_def = &enum_def.variants[v.data.tag];
                let offset = self.data.offset;
                let field = variant_def.fields.get(offset).expect("Bad field offset");
                Some(m.identifier_at(field.name).to_owned())
            }
        }
    }

    /// Gets the type of this field.
    pub fn get_type(&self) -> Type {
        match &self.data.info {
            FieldInfo::DeclaredStruct { def_idx } => {
                let struct_def = self
                    .parent_env
                    .module_env()
                    .data
                    .module
                    .struct_def_at(*def_idx);
                let field = match &struct_def.field_information {
                    StructFieldInformation::Declared(fields) => &fields[self.data.offset],
                    StructFieldInformation::Native => unreachable!(),
                };
                self.parent_env
                    .module_env()
                    .globalize_signature(&field.signature.0)
            }
            FieldInfo::DeclaredEnum { def_idx } => {
                let EnclosingEnv::Variant(v) = &self.parent_env else {
                    unreachable!()
                };
                let enum_def = v.enum_env.module_env.data.module.enum_def_at(*def_idx);
                let variant_def = &enum_def.variants[v.data.tag];
                let field = &variant_def.fields[self.data.offset];
                v.enum_env
                    .module_env
                    .globalize_signature(&field.signature.0)
            }
        }
    }

    /// Get field offset.
    pub fn get_offset(&self) -> usize {
        self.data.offset
    }
}

// =================================================================================================
/// # Named Constant Environment

#[derive(Debug)]
pub struct NamedConstantData {
    /// The name of this constant
    name: Symbol,

    /// The location of this constant
    loc: Loc,

    /// The type of this constant
    typ: Type,

    /// The value of this constant
    value: Value,

    /// Attributes attached to this constant
    attributes: Vec<Attribute>,
}

#[derive(Debug)]
pub struct NamedConstantEnv<'env> {
    /// Reference to enclosing module.
    pub module_env: ModuleEnv<'env>,

    data: &'env NamedConstantData,
}

impl<'env> NamedConstantEnv<'env> {
    /// Returns the name of this constant
    pub fn get_name(&self) -> Symbol {
        self.data.name
    }

    /// Returns the id of this constant
    pub fn get_id(&self) -> NamedConstantId {
        NamedConstantId(self.data.name)
    }

    /// Returns the location of this constant
    pub fn get_loc(&self) -> Loc {
        self.data.loc.clone()
    }

    /// Returns the type of the constant
    pub fn get_type(&self) -> Type {
        self.data.typ.clone()
    }

    /// Returns the value of this constant
    pub fn get_value(&self) -> Value {
        self.data.value.clone()
    }

    /// Returns the attributes attached to this constant
    pub fn get_attributes(&self) -> &[Attribute] {
        &self.data.attributes
    }
}

// =================================================================================================
/// # Function Environment

/// Represents a type parameter.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TypeParameter(pub Symbol, pub AbilityConstraint);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct AbilityConstraint(pub AbilitySet);

/// Represents a parameter.
#[derive(Debug, Clone)]
pub struct Parameter(pub Symbol, pub Type);

#[derive(Debug)]
pub struct FunctionData {
    /// Name of this function.
    name: Symbol,

    /// Location of this function.
    loc: Loc,

    /// The definition index of this function in its module.
    def_idx: FunctionDefinitionIndex,

    /// The handle index of this function in its module.
    handle_idx: FunctionHandleIndex,

    /// Attributes attached to this function.
    attributes: Vec<Attribute>,

    /// List of function argument names. Not in bytecode but obtained from AST.
    arg_names: Vec<Symbol>,

    /// List of type argument names. Not in bytecode but obtained from AST.
    #[allow(unused)]
    type_arg_names: Vec<Symbol>,

    /// A cache for the called functions.
    called_funs: RefCell<Option<BTreeSet<QualifiedId<FunId>>>>,

    /// A cache for the calling functions.
    calling_funs: RefCell<Option<BTreeSet<QualifiedId<FunId>>>>,

    /// A cache for the transitive closure of the called functions.
    transitive_closure_of_called_funs: RefCell<Option<BTreeSet<QualifiedId<FunId>>>>,
}

impl FunctionData {
    pub fn stub(
        name: Symbol,
        def_idx: FunctionDefinitionIndex,
        handle_idx: FunctionHandleIndex,
    ) -> Self {
        FunctionData {
            name,
            loc: Loc::default(),
            attributes: Vec::default(),
            def_idx,
            handle_idx,
            arg_names: vec![],
            type_arg_names: vec![],
            called_funs: Default::default(),
            calling_funs: Default::default(),
            transitive_closure_of_called_funs: Default::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FunctionEnv<'env> {
    /// Reference to enclosing module.
    pub module_env: ModuleEnv<'env>,

    /// Reference to the function data.
    data: &'env FunctionData,
}

impl<'env> FunctionEnv<'env> {
    /// Returns the name of this function.
    pub fn get_name(&self) -> Symbol {
        self.data.name
    }

    /// Gets full name as string.
    pub fn get_full_name_str(&self) -> String {
        format!(
            "{}::{}",
            self.module_env.get_name().display(self.symbol_pool()),
            self.get_name_str()
        )
    }

    pub fn get_name_str(&self) -> String {
        self.get_name().display(self.symbol_pool()).to_string()
    }

    /// Returns the VM identifier for this function
    pub fn get_identifier(&'env self) -> Identifier {
        let m = &self.module_env.data.module;
        m.identifier_at(m.function_handle_at(self.data.handle_idx).name)
            .to_owned()
    }

    /// Gets the id of this function.
    pub fn get_id(&self) -> FunId {
        FunId(self.data.name)
    }

    /// Gets the qualified id of this function.
    pub fn get_qualified_id(&self) -> QualifiedId<FunId> {
        self.module_env.get_id().qualified(self.get_id())
    }

    /// Gets the definition index of this function.
    pub fn get_def_idx(&self) -> FunctionDefinitionIndex {
        self.data.def_idx
    }

    /// Shortcut for accessing the symbol pool.
    pub fn symbol_pool(&self) -> &SymbolPool {
        self.module_env.symbol_pool()
    }

    /// Returns the location of this function.
    pub fn get_loc(&self) -> Loc {
        self.data.loc.clone()
    }

    /// Returns the attributes of this function.
    pub fn get_attributes(&self) -> &[Attribute] {
        &self.data.attributes
    }

    /// Returns the location of the bytecode at the given offset.
    pub fn get_bytecode_loc(&self, offset: u16) -> Loc {
        if let Ok(fmap) = self
            .module_env
            .data
            .source_map
            .get_function_source_map(self.data.def_idx)
        {
            if let Some(loc) = fmap.get_code_location(offset) {
                return self.module_env.env.to_loc(&loc);
            }
        }
        self.get_loc()
    }

    /// Returns the bytecode associated with this function.
    pub fn get_bytecode(&self) -> &[Bytecode] {
        let function_definition = self
            .module_env
            .data
            .module
            .function_def_at(self.get_def_idx());
        match &function_definition.code {
            Some(code) => &code.code,
            None => &[],
        }
    }

    /// Returns the variant jump tables for this function.
    pub fn get_jump_tables(&self) -> &[VariantJumpTable] {
        let function_definition = self
            .module_env
            .data
            .module
            .function_def_at(self.get_def_idx());
        &function_definition.code.as_ref().unwrap().jump_tables
    }

    /// Returns true if this function is native.
    pub fn is_native(&self) -> bool {
        self.definition().is_native()
    }

    /// Returns true if this is the well-known native or intrinsic function of the given name.
    /// The function must reside either in stdlib or extlib address domain.
    pub fn is_well_known(&self, name: &str) -> bool {
        let env = self.module_env.env;
        if !self.is_native() {
            return false;
        }
        let addr = self.module_env.get_name().addr();
        (addr == &env.get_stdlib_address() || addr == &env.get_extlib_address())
            && self.get_full_name_str() == name
    }

    /// Return the visibility of this function
    pub fn visibility(&self) -> FunctionVisibility {
        self.definition().visibility
    }

    /// Return true if the function is an entry fucntion
    pub fn is_entry(&self) -> bool {
        self.definition().is_entry
    }

    /// Return the visibility string for this function. Useful for formatted printing.
    pub fn visibility_str(&self) -> &str {
        match self.visibility() {
            Visibility::Public => "public ",
            Visibility::Friend => "public(friend) ",
            Visibility::Private => "",
        }
    }

    /// Return whether this function is exposed outside of the module.
    pub fn is_exposed(&self) -> bool {
        self.module_env.is_script_module()
            || self.definition().is_entry
            || match self.definition().visibility {
                Visibility::Public | Visibility::Friend => true,
                Visibility::Private => false,
            }
    }

    /// Return whether this function is exposed outside of the module.
    pub fn has_unknown_callers(&self) -> bool {
        self.module_env.is_script_module()
            || self.definition().is_entry
            || match self.definition().visibility {
                Visibility::Public => true,
                Visibility::Private | Visibility::Friend => false,
            }
    }

    /// Returns true if the function is a script function
    pub fn is_script(&self) -> bool {
        // The main function of a scipt is a script function
        self.module_env.is_script_module() || self.definition().is_entry
    }

    /// Return true if this function is a friend function
    pub fn is_friend(&self) -> bool {
        self.definition().visibility == Visibility::Friend
    }

    /// Returns true if this function mutates any references (i.e. has &mut parameters).
    pub fn is_mutating(&self) -> bool {
        self.get_parameters()
            .iter()
            .any(|Parameter(_, ty)| ty.is_mutable_reference())
    }

    /// Returns the type parameters associated with this function.
    pub fn get_type_parameters(&self) -> Vec<TypeParameter> {
        // TODO: currently the translation scheme isn't working with using real type
        //   parameter names, so use indices instead.
        let fdef = self.definition();
        let fhandle = self
            .module_env
            .data
            .module
            .function_handle_at(fdef.function);
        fhandle
            .type_parameters
            .iter()
            .enumerate()
            .map(|(i, k)| {
                TypeParameter(
                    self.module_env.env.symbol_pool.make(&format!("$tv{}", i)),
                    AbilityConstraint(*k),
                )
            })
            .collect_vec()
    }

    /// Returns the type parameters with the real names.
    pub fn get_named_type_parameters(&self) -> Vec<TypeParameter> {
        let fdef = self.definition();
        let fhandle = self
            .module_env
            .data
            .module
            .function_handle_at(fdef.function);
        fhandle
            .type_parameters
            .iter()
            .enumerate()
            .map(|(i, k)| {
                let name = self
                    .module_env
                    .data
                    .source_map
                    .get_function_source_map(self.data.def_idx)
                    .ok()
                    .and_then(|fmap| fmap.type_parameters.get(i))
                    .map(|(s, _)| s.clone())
                    .unwrap_or_else(|| format!("unknown#{}", i));
                TypeParameter(
                    self.module_env.env.symbol_pool.make(&name),
                    AbilityConstraint(*k),
                )
            })
            .collect_vec()
    }

    pub fn get_parameter_count(&self) -> usize {
        let fdef = self.definition();
        let module = &self.module_env.data.module;
        let fhandle = module.function_handle_at(fdef.function);
        module.signature_at(fhandle.parameters).0.len()
    }

    /// Return the number of type parameters for self
    pub fn get_type_parameter_count(&self) -> usize {
        let fdef = self.definition();
        let fhandle = self
            .module_env
            .data
            .module
            .function_handle_at(fdef.function);
        fhandle.type_parameters.len()
    }

    /// Return `true` if idx is a formal parameter index
    pub fn is_parameter(&self, idx: usize) -> bool {
        idx < self.get_parameter_count()
    }

    /// Return true if this is a named parameter of this function.
    pub fn is_named_parameter(&self, name: &str) -> bool {
        self.get_parameters()
            .iter()
            .any(|p| self.symbol_pool().string(p.0).as_ref() == name)
    }

    /// Returns the parameter types associated with this function
    pub fn get_parameter_types(&self) -> Vec<Type> {
        let fdef = self.definition();
        let module = &self.module_env.data.module;
        let fhandle = module.function_handle_at(fdef.function);
        module
            .signature_at(fhandle.parameters)
            .0
            .iter()
            .map(|tv: &SignatureToken| self.module_env.globalize_signature(tv))
            .collect()
    }

    /// Returns the regular parameters associated with this function.
    pub fn get_parameters(&self) -> Vec<Parameter> {
        let fdef = self.definition();
        let module = &self.module_env.data.module;
        let fhandle = module.function_handle_at(fdef.function);
        module
            .signature_at(fhandle.parameters)
            .0
            .iter()
            .map(|tv: &SignatureToken| self.module_env.globalize_signature(tv))
            .zip(self.data.arg_names.iter())
            .map(|(s, i)| Parameter(*i, s))
            .collect_vec()
    }

    /// Returns return types of this function.
    pub fn get_return_types(&self) -> Vec<Type> {
        let fdef = self.definition();
        let module = &self.module_env.data.module;
        let fhandle = module.function_handle_at(fdef.function);
        module
            .signature_at(fhandle.return_)
            .0
            .iter()
            .map(|tv: &SignatureToken| self.module_env.globalize_signature(tv))
            .collect_vec()
    }

    /// Returns return type at given index.
    pub fn get_return_type(&self, idx: usize) -> Type {
        self.get_return_types()[idx].clone()
    }

    /// Returns the number of return values of this function.
    pub fn get_return_count(&self) -> usize {
        let fdef = self.definition();
        let module = &self.module_env.data.module;
        let fhandle = module.function_handle_at(fdef.function);
        module.signature_at(fhandle.return_).0.len()
    }

    /// Get the name to be used for a local. If the local is an argument, use that for naming,
    /// otherwise generate a unique name.
    pub fn get_local_name(&self, idx: usize) -> Symbol {
        if idx < self.data.arg_names.len() {
            return self.data.arg_names[idx];
        }
        // Try to obtain name from source map.
        if let Ok(fmap) = self
            .module_env
            .data
            .source_map
            .get_function_source_map(self.data.def_idx)
        {
            if let Some((ident, _)) = fmap.get_parameter_or_local_name(idx as u64) {
                // The Move compiler produces temporary names of the form `<foo>%#<num>`,
                // where <num> seems to be generated non-deterministically.
                // Substitute this by a deterministic name which the backend accepts.
                let clean_ident = if ident.contains("%#") {
                    format!("tmp#${}", idx)
                } else {
                    ident
                };
                return self.module_env.env.symbol_pool.make(clean_ident.as_str());
            }
        }
        self.module_env.env.symbol_pool.make(&format!("$t{}", idx))
    }

    /// Returns true if the index is for a temporary, not user declared local.
    pub fn is_temporary(&self, idx: usize) -> bool {
        if idx >= self.get_local_count() {
            return true;
        }
        let name = self.get_local_name(idx);
        self.symbol_pool().string(name).contains("tmp#$")
    }

    /// Gets the number of proper locals of this function. Those are locals which are declared
    /// by the user and also have a user assigned name which can be discovered via `get_local_name`.
    /// Note we may have more anonymous locals generated e.g by the 'stackless' transformation.
    pub fn get_local_count(&self) -> usize {
        let fdef = self.definition();
        let module = &self.module_env.data.module;
        let num_params = self.get_parameter_count();
        let num_locals = fdef
            .code
            .as_ref()
            .map(|code| module.signature_at(code.locals).0.len())
            .unwrap_or(0);
        num_params + num_locals
    }

    /// Gets the type of the local at index. This must use an index in the range as determined by
    /// `get_local_count`.
    pub fn get_local_type(&self, idx: usize) -> Type {
        let fdef = self.definition();
        let module = &self.module_env.data.module;
        let fhandle = module.function_handle_at(fdef.function);
        let parameters = &module.signature_at(fhandle.parameters).0;
        let st = if idx < parameters.len() {
            &parameters[idx]
        } else {
            let locals = &module.signature_at(fdef.code.as_ref().unwrap().locals).0;
            &locals[idx - parameters.len()]
        };
        self.module_env.globalize_signature(st)
    }

    /// Returns the acquired global resource types.
    pub fn get_acquires_global_resources(&'env self) -> Vec<DatatypeId> {
        let function_definition = self
            .module_env
            .data
            .module
            .function_def_at(self.get_def_idx());
        function_definition
            .acquires_global_resources
            .iter()
            .map(|x| self.module_env.get_struct_id(*x))
            .collect()
    }

    /// Returns true if either the name or simple name of this function matches the given string
    pub fn matches_name(&self, name: &str) -> bool {
        name.eq(&*self.get_simple_name_string()) || name.eq(&*self.get_name_string())
    }

    /// Get the functions that call this one
    pub fn get_calling_functions(&self) -> BTreeSet<QualifiedId<FunId>> {
        if let Some(calling) = &*self.data.calling_funs.borrow() {
            return calling.clone();
        }
        let mut set: BTreeSet<QualifiedId<FunId>> = BTreeSet::new();
        for module_env in self.module_env.env.get_modules() {
            for fun_env in module_env.get_functions() {
                if fun_env
                    .get_called_functions()
                    .contains(&self.get_qualified_id())
                {
                    set.insert(fun_env.get_qualified_id());
                }
            }
        }
        *self.data.calling_funs.borrow_mut() = Some(set.clone());
        set
    }

    /// Get the functions that this one calls
    pub fn get_called_functions(&self) -> BTreeSet<QualifiedId<FunId>> {
        if let Some(called) = &*self.data.called_funs.borrow() {
            return called.clone();
        }
        let called: BTreeSet<_> = self
            .get_bytecode()
            .iter()
            .filter_map(|c| {
                if let Bytecode::Call(i) = c {
                    Some(self.module_env.get_used_function(*i).get_qualified_id())
                } else if let Bytecode::CallGeneric(i) = c {
                    let handle_idx = self
                        .module_env
                        .data
                        .module
                        .function_instantiation_at(*i)
                        .handle;
                    Some(
                        self.module_env
                            .get_used_function(handle_idx)
                            .get_qualified_id(),
                    )
                } else {
                    None
                }
            })
            .collect();
        *self.data.called_funs.borrow_mut() = Some(called.clone());
        called
    }

    /// Get the transitive closure of the called functions
    pub fn get_transitive_closure_of_called_functions(&self) -> BTreeSet<QualifiedId<FunId>> {
        if let Some(trans_called) = &*self.data.transitive_closure_of_called_funs.borrow() {
            return trans_called.clone();
        }

        let mut set = BTreeSet::new();
        let mut reachable_funcs = VecDeque::new();
        reachable_funcs.push_back(self.clone());

        // BFS in reachable_funcs to collect all reachable functions
        while !reachable_funcs.is_empty() {
            let current_fnc = reachable_funcs.pop_front();
            if let Some(fnc) = current_fnc {
                for callee in fnc.get_called_functions() {
                    let f = self.module_env.env.get_function(callee);
                    let qualified_id = f.get_qualified_id();
                    if !set.contains(&qualified_id) {
                        set.insert(qualified_id);
                        reachable_funcs.push_back(f.clone());
                    }
                }
            }
        }
        *self.data.transitive_closure_of_called_funs.borrow_mut() = Some(set.clone());
        set
    }

    /// Returns the function name excluding the address and the module name
    pub fn get_simple_name_string(&self) -> Rc<String> {
        self.symbol_pool().string(self.get_name())
    }

    /// Returns the function name with the module name excluding the address
    pub fn get_name_string(&self) -> Rc<str> {
        if self.module_env.is_script_module() {
            Rc::from(format!("Script::{}", self.get_simple_name_string()))
        } else {
            let module_name = self
                .module_env
                .get_name()
                .display(self.module_env.symbol_pool());
            Rc::from(format!(
                "{}::{}",
                module_name,
                self.get_simple_name_string()
            ))
        }
    }

    fn definition(&'env self) -> &'env FunctionDefinition {
        self.module_env
            .data
            .module
            .function_def_at(self.data.def_idx)
    }

    /// Produce a TypeDisplayContext to print types within the scope of this env
    pub fn get_type_display_ctx(&self) -> TypeDisplayContext {
        let type_param_names = self
            .get_type_parameters()
            .iter()
            .map(|param| param.0)
            .collect();
        TypeDisplayContext::WithEnv {
            env: self.module_env.env,
            type_param_names: Some(type_param_names),
        }
    }

    /// Returns the object types that may be shared by this function
    /// If `transitive` is false, only return objects directly shared by this function
    /// If `transitive` is true, return objects shared by both this function and its transitive callees
    pub fn get_shared_objects(&'env self, transitive: bool) -> BTreeSet<Type> {
        let mut shared = BTreeSet::new();
        if transitive {
            let callees = self.get_transitive_closure_of_called_functions();
            for callee in callees {
                let fenv = self.module_env.env.get_function(callee);
                shared.extend(fenv.get_shared_objects(false));
            }
        } else {
            let module = &self.module_env.data.module;
            for b in self.get_bytecode() {
                if let Bytecode::CallGeneric(fi_idx) = b {
                    let FunctionInstantiation {
                        handle,
                        type_parameters,
                    } = module.function_instantiation_at(*fi_idx);
                    let f_ref = FunctionRef::from_idx(module, handle);
                    if is_framework_function(
                        &f_ref,
                        "transfer",
                        vec!["share_object", "public_share_object"],
                    ) {
                        let type_params = module.signature_at(*type_parameters);
                        shared.insert(self.module_env.globalize_signature(&type_params.0[0]));
                    }
                }
            }
        }

        shared
    }

    /// Returns the object types that may be transferred by this function
    /// If `transitive` is false, only objects directly transferred by this function
    /// If `transitive` is true, return objects transferred by both this function and its transitive callees
    pub fn get_transferred_objects(&'env self, transitive: bool) -> BTreeSet<Type> {
        let mut transferred = BTreeSet::new();
        if transitive {
            let callees = self.get_transitive_closure_of_called_functions();
            for callee in callees {
                let fenv = self.module_env.env.get_function(callee);
                transferred.extend(fenv.get_shared_objects(false));
            }
        } else {
            let module = &self.module_env.data.module;
            for b in self.get_bytecode() {
                if let Bytecode::CallGeneric(fi_idx) = b {
                    let FunctionInstantiation {
                        handle,
                        type_parameters,
                    } = module.function_instantiation_at(*fi_idx);
                    let f_ref = FunctionRef::from_idx(module, handle);
                    if is_framework_function(
                        &f_ref,
                        "transfer",
                        vec!["transfer", "public_transfer"],
                    ) {
                        let type_params = module.signature_at(*type_parameters);
                        transferred.insert(self.module_env.globalize_signature(&type_params.0[0]));
                    }
                }
            }
        }

        transferred
    }

    /// Returns the object types that may be frozen by this function
    /// If `transitive` is false, only return objects directly frozen by this function
    /// If `transitive` is true, return objects frozen by both this function and its transitive callees
    pub fn get_frozen_objects(&'env self, transitive: bool) -> BTreeSet<Type> {
        let mut frozen = BTreeSet::new();
        if transitive {
            let callees = self.get_transitive_closure_of_called_functions();
            for callee in callees {
                let fenv = self.module_env.env.get_function(callee);
                frozen.extend(fenv.get_shared_objects(false));
            }
        } else {
            let module = &self.module_env.data.module;
            for b in self.get_bytecode() {
                if let Bytecode::CallGeneric(fi_idx) = b {
                    let FunctionInstantiation {
                        handle,
                        type_parameters,
                    } = module.function_instantiation_at(*fi_idx);
                    let f_ref = FunctionRef::from_idx(module, handle);
                    if is_framework_function(
                        &f_ref,
                        "transfer",
                        vec!["freeze_object", "public_freeze_object"],
                    ) {
                        let type_params = module.signature_at(*type_parameters);
                        frozen.insert(self.module_env.globalize_signature(&type_params.0[0]));
                    }
                }
            }
        }

        frozen
    }

    /// Returns the event types that may be emitted by this function
    /// If `transitive` is false, only return events directly emitted by this function
    /// If `transitive` is true, return events emitted by both this function and its transitive callees
    pub fn get_events(&'env self, transitive: bool) -> BTreeSet<Type> {
        let mut events = BTreeSet::new();
        if transitive {
            let callees = self.get_transitive_closure_of_called_functions();
            for callee in callees {
                let fenv = self.module_env.env.get_function(callee);
                events.extend(fenv.get_events(false));
            }
        } else {
            let module = &self.module_env.data.module;
            for b in self.get_bytecode() {
                if let Bytecode::CallGeneric(fi_idx) = b {
                    let FunctionInstantiation {
                        handle,
                        type_parameters,
                    } = module.function_instantiation_at(*fi_idx);
                    let f_ref = FunctionRef::from_idx(module, handle);
                    if is_framework_function(&f_ref, "event", vec!["emit"]) {
                        let type_params = module.signature_at(*type_parameters);
                        events.insert(self.module_env.globalize_signature(&type_params.0[0]));
                    }
                }
            }
        }

        events
    }
}

// =================================================================================================
/// # Expression Environment

/// Represents context for an expression.
#[derive(Debug, Clone)]
pub struct ExpInfo {
    /// The associated location of this expression.
    loc: Loc,
    /// The type of this expression.
    ty: Type,
    /// The associated instantiation of type parameters for this expression, if applicable
    instantiation: Option<Vec<Type>>,
}

impl ExpInfo {
    pub fn new(loc: Loc, ty: Type) -> Self {
        ExpInfo {
            loc,
            ty,
            instantiation: None,
        }
    }
}

// =================================================================================================
/// # Formatting

pub struct LocDisplay<'env> {
    loc: &'env Loc,
    env: &'env GlobalEnv,
    only_line: bool,
}

impl Loc {
    pub fn display<'env>(&'env self, env: &'env GlobalEnv) -> LocDisplay<'env> {
        LocDisplay {
            loc: self,
            env,
            only_line: false,
        }
    }

    pub fn display_line_only<'env>(&'env self, env: &'env GlobalEnv) -> LocDisplay<'env> {
        LocDisplay {
            loc: self,
            env,
            only_line: true,
        }
    }
}

impl<'env> fmt::Display for LocDisplay<'env> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some((fname, pos)) = self.env.get_file_and_location(self.loc) {
            if self.only_line {
                write!(f, "at {}:{}", fname, pos.line + LineOffset(1))
            } else {
                let offset = self.loc.span.end() - self.loc.span.start();
                write!(
                    f,
                    "at {}:{}:{}+{}",
                    fname,
                    pos.line + LineOffset(1),
                    pos.column + ColumnOffset(1),
                    offset,
                )
            }
        } else {
            write!(f, "{:?}", self.loc)
        }
    }
}

pub trait GetNameString {
    fn get_name_for_display(&self, env: &GlobalEnv) -> String;
}

impl GetNameString for QualifiedId<DatatypeId> {
    fn get_name_for_display(&self, env: &GlobalEnv) -> String {
        env.get_struct_qid(*self).get_full_name_str()
    }
}

impl GetNameString for QualifiedId<FunId> {
    fn get_name_for_display(&self, env: &GlobalEnv) -> String {
        env.get_function_qid(*self).get_full_name_str()
    }
}

impl<'a, Id: Clone> fmt::Display for EnvDisplay<'a, QualifiedId<Id>>
where
    QualifiedId<Id>: GetNameString,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.val.get_name_for_display(self.env))
    }
}

impl<'a, Id: Clone> fmt::Display for EnvDisplay<'a, QualifiedInstId<Id>>
where
    QualifiedId<Id>: GetNameString,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.env.display(&self.val.to_qualified_id()))?;
        if !self.val.inst.is_empty() {
            let tctx = TypeDisplayContext::WithEnv {
                env: self.env,
                type_param_names: None,
            };
            write!(f, "<")?;
            let mut sep = "";
            for ty in &self.val.inst {
                write!(f, "{}{}", sep, ty.display(&tctx))?;
                sep = ", ";
            }
            write!(f, ">")?;
        }
        Ok(())
    }
}
