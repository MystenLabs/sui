// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use lru::LruCache;
use move_binary_format::file_format::{
    AbilitySet, DatatypeTyParameter, EnumDefinitionIndex, FunctionDefinitionIndex,
    Signature as MoveSignature, SignatureIndex, Visibility,
};
use move_command_line_common::display::RenderResult;
use move_command_line_common::{display::try_render_constant, error_bitset::ErrorBitset};
use move_core_types::annotated_value::MoveEnumLayout;
use move_core_types::language_storage::ModuleId;
use std::collections::BTreeSet;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use std::{borrow::Cow, collections::BTreeMap};
use sui_types::base_types::is_primitive_type_tag;
use sui_types::transaction::{Argument, CallArg, Command, ProgrammableTransaction};
use sui_types::type_input::{StructInput, TypeInput};

use crate::error::Error;
use move_binary_format::errors::Location;
use move_binary_format::{
    file_format::{
        DatatypeHandleIndex, SignatureToken, StructDefinitionIndex, StructFieldInformation,
        TableIndex,
    },
    CompiledModule,
};
use move_core_types::{
    account_address::AccountAddress,
    annotated_value::{MoveFieldLayout, MoveStructLayout, MoveTypeLayout},
    language_storage::{StructTag, TypeTag},
};
use sui_types::move_package::{MovePackage, TypeOrigin};
use sui_types::object::Object;
use sui_types::{base_types::SequenceNumber, Identifier};

pub mod error;

// TODO Move to ServiceConfig

const PACKAGE_CACHE_SIZE: NonZeroUsize = unsafe { NonZeroUsize::new_unchecked(1024) };

pub type Result<T> = std::result::Result<T, Error>;

/// The Resolver is responsible for providing information about types. It relies on its internal
/// `package_store` to load packages and then type definitions from those packages.
#[derive(Debug)]
pub struct Resolver<S> {
    package_store: S,
    limits: Option<Limits>,
}

/// Optional configuration that imposes limits on the work that the resolver can do for each
/// request.
#[derive(Debug)]
pub struct Limits {
    /// Maximum recursion depth through type parameters.
    pub max_type_argument_depth: usize,
    /// Maximum number of type arguments in a single type instantiation.
    pub max_type_argument_width: usize,
    /// Maximum size for the resolution context.
    pub max_type_nodes: usize,
    /// Maximum recursion depth through struct fields.
    pub max_move_value_depth: usize,
}

/// Store which fetches package for the given address from the backend db and caches it
/// locally in an lru cache. On every call to `fetch` it checks backend db and if package
/// version is stale locally, it updates the local state before returning to the user
pub struct PackageStoreWithLruCache<T> {
    pub(crate) packages: Mutex<LruCache<AccountAddress, Arc<Package>>>,
    pub(crate) inner: T,
}

#[derive(Clone, Debug)]
pub struct Package {
    /// The ID this package was loaded from on-chain.
    storage_id: AccountAddress,

    /// The ID that this package is associated with at runtime.  Bytecode in other packages refers
    /// to types and functions from this package using this ID.
    runtime_id: AccountAddress,

    /// The package's transitive dependencies as a mapping from the package's runtime ID (the ID it
    /// is referred to by in other packages) to its storage ID (the ID it is loaded from on chain).
    linkage: Linkage,

    /// The version this package was loaded at -- necessary for handling race conditions when
    /// loading system packages.
    version: SequenceNumber,

    modules: BTreeMap<String, Module>,
}

type Linkage = BTreeMap<AccountAddress, AccountAddress>;

/// A `CleverError` is a special kind of abort code that is used to encode more information than a
/// normal abort code. These clever errors are used to encode the line number, error constant name,
/// and error constant value as pool indicies packed into a format satisfying the `ErrorBitset`
/// format. This struct is the "inflated" view of that data, providing the module ID, line number,
/// and error constant name and value (if available).
#[derive(Clone, Debug)]
pub struct CleverError {
    /// The (storage) module ID of the module that the assertion failed in.
    pub module_id: ModuleId,
    /// Inner error information. This is either a complete error, just a line number, or bytes that
    /// should be treated opaquely.
    pub error_info: ErrorConstants,
    /// The line number in the source file where the error occured.
    pub source_line_number: u16,
}

/// The `ErrorConstants` enum is used to represent the different kinds of error information that
/// can be returned from a clever error when looking at the constant values for the clever error.
/// These values are either:
/// * `None` - No constant information is available, only a line number.
/// * `Rendered` - The error is a complete error, with an error identifier and constant that can be
///    rendered in a human-readable format (see in-line doc comments for exact types of values
///    supported).
/// * `Raw` - If there is an error constant value, but it is not a renderable type (e.g., a
///   `vector<address>`), then it is treated as opaque and the bytes are returned.
#[derive(Clone, Debug)]
pub enum ErrorConstants {
    /// No constant information is available, only a line number.
    None,
    /// The error is a complete error, with an error identifier and constant that can be rendered.
    /// The rendered string representation of the constant is returned only when the contant
    /// value is one of the following types:
    /// * A vector of bytes convertible to a valid UTF-8 string; or
    /// * A numeric value (u8, u16, u32, u64, u128, u256); or
    /// * A boolean value; or
    /// * An address value
    ///
    /// Otherwise, the `Raw` bytes of the error constant are returned.
    Rendered {
        /// The name of the error constant.
        identifier: String,
        /// The value of the error constant.
        constant: String,
    },
    /// If there is an error constant value, but ii is not one of the above types, then it is
    /// treated as opaque and the bytes are returned. The caller is responsible for determining how
    /// best to display the error constant in this case.
    Raw {
        /// The name of the error constant.
        identifier: String,
        /// The raw (BCS) bytes of the error constant.
        bytes: Vec<u8>,
    },
}

#[derive(Clone, Debug)]
pub struct Module {
    bytecode: CompiledModule,

    /// Index mapping struct names to their defining ID, and the index for their definition in the
    /// bytecode, to speed up definition lookups.
    struct_index: BTreeMap<String, (AccountAddress, StructDefinitionIndex)>,

    /// Index mapping enum names to their defining ID and the index of their definition in the
    /// bytecode. This speeds up definition lookups.
    enum_index: BTreeMap<String, (AccountAddress, EnumDefinitionIndex)>,

    /// Index mapping function names to the index for their definition in the bytecode, to speed up
    /// definition lookups.
    function_index: BTreeMap<String, FunctionDefinitionIndex>,
}

/// Deserialized representation of a struct definition.
#[derive(Debug)]
pub struct DataDef {
    /// The storage ID of the package that first introduced this type.
    pub defining_id: AccountAddress,

    /// This type's abilities.
    pub abilities: AbilitySet,

    /// Ability constraints and phantom status for type parameters
    pub type_params: Vec<DatatypeTyParameter>,

    /// The internal data of the datatype. This can either be a sequence of fields, or a sequence
    /// of variants.
    pub data: MoveData,
}

#[derive(Debug)]
pub enum MoveData {
    /// Serialized representation of fields (names and deserialized signatures). Signatures refer to
    /// packages at their runtime IDs (not their storage ID or defining ID).
    Struct(Vec<(String, OpenSignatureBody)>),

    /// Serialized representation of variants (names and deserialized signatures).
    Enum(Vec<VariantDef>),
}

/// Deserialized representation of an enum definition. These are always held inside an `EnumDef`.
#[derive(Debug)]
pub struct VariantDef {
    /// The name of the enum variant
    pub name: String,

    /// The serialized representation of the variant's signature. Signatures refer to packages at
    /// their runtime IDs (not their storage ID or defining ID).
    pub signatures: Vec<(String, OpenSignatureBody)>,
}

/// Deserialized representation of a function definition
#[derive(Debug)]
pub struct FunctionDef {
    /// Whether the function is `public`, `private` or `public(friend)`.
    pub visibility: Visibility,

    /// Whether the function is marked `entry` or not.
    pub is_entry: bool,

    /// Ability constraints for type parameters
    pub type_params: Vec<AbilitySet>,

    /// Formal parameter types.
    pub parameters: Vec<OpenSignature>,

    /// Return types.
    pub return_: Vec<OpenSignature>,
}

/// Fully qualified struct identifier.  Uses copy-on-write strings so that when it is used as a key
/// to a map, an instance can be created to query the map without having to allocate strings on the
/// heap.
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone, Hash)]
pub struct DatatypeRef<'m, 'n> {
    pub package: AccountAddress,
    pub module: Cow<'m, str>,
    pub name: Cow<'n, str>,
}

/// A `StructRef` that owns its strings.
pub type DatatypeKey = DatatypeRef<'static, 'static>;

#[derive(Copy, Clone, Debug)]
pub enum Reference {
    Immutable,
    Mutable,
}

/// A function parameter or return signature, with its type parameters instantiated.
#[derive(Clone, Debug)]
pub struct Signature {
    pub ref_: Option<Reference>,
    pub body: TypeTag,
}

/// Deserialized representation of a type signature that could appear as a function parameter or
/// return.
#[derive(Clone, Debug)]
pub struct OpenSignature {
    pub ref_: Option<Reference>,
    pub body: OpenSignatureBody,
}

/// Deserialized representation of a type signature that could appear as a field type for a struct.
#[derive(Clone, Debug)]
pub enum OpenSignatureBody {
    Address,
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
    Vector(Box<OpenSignatureBody>),
    Datatype(DatatypeKey, Vec<OpenSignatureBody>),
    TypeParameter(u16),
}

/// Information necessary to convert a type tag into a type layout.
#[derive(Debug, Default)]
struct ResolutionContext<'l> {
    /// Definitions (field information) for structs referred to by types added to this context.
    datatypes: BTreeMap<DatatypeKey, DataDef>,

    /// Limits configuration from the calling resolver.
    limits: Option<&'l Limits>,
}

/// Interface to abstract over access to a store of live packages.  Used to override the default
/// store during testing.
#[async_trait]
pub trait PackageStore: Send + Sync + 'static {
    /// Read package contents. Fails if `id` is not an object, not a package, or is malformed in
    /// some way.
    async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>>;
}

macro_rules! as_ref_impl {
    ($type:ty) => {
        #[async_trait]
        impl PackageStore for $type {
            async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>> {
                self.as_ref().fetch(id).await
            }
        }
    };
}

as_ref_impl!(Arc<dyn PackageStore>);
as_ref_impl!(Box<dyn PackageStore>);

/// Check $value does not exceed $limit in config, if the limit config exists, returning an error
/// containing the max value and actual value otherwise.
macro_rules! check_max_limit {
    ($err:ident, $config:expr; $limit:ident $op:tt $value:expr) => {
        if let Some(l) = $config {
            let max = l.$limit;
            let val = $value;
            if !(max $op val) {
                return Err(Error::$err(max, val));
            }
        }
    };
}

impl<S> Resolver<S> {
    pub fn new(package_store: S) -> Self {
        Self {
            package_store,
            limits: None,
        }
    }

    pub fn new_with_limits(package_store: S, limits: Limits) -> Self {
        Self {
            package_store,
            limits: Some(limits),
        }
    }

    pub fn package_store(&self) -> &S {
        &self.package_store
    }

    pub fn package_store_mut(&mut self) -> &mut S {
        &mut self.package_store
    }
}

impl<S: PackageStore> Resolver<S> {
    /// The canonical form of a type refers to each type in terms of its defining package ID. This
    /// function takes a non-canonical type and updates all its package IDs to the appropriate
    /// defining ID.
    ///
    /// For every `package::module::datatype` in the input `tag`, `package` must be an object
    /// on-chain, containing a move package that includes `module`, and that module must define the
    /// `datatype`. In practice this means the input type `tag` can refer to types at or after
    /// their defining IDs.
    pub async fn canonical_type(&self, mut tag: TypeTag) -> Result<TypeTag> {
        let mut context = ResolutionContext::new(self.limits.as_ref());

        // (1). Fetch all the information from this store that is necessary to relocate package IDs
        // in the type.
        context
            .add_type_tag(
                &mut tag,
                &self.package_store,
                /* visit_fields */ false,
                /* visit_phantoms */ true,
            )
            .await?;

        // (2). Use that information to relocate package IDs in the type.
        context.canonicalize_type(&mut tag)?;
        Ok(tag)
    }

    /// Return the type layout corresponding to the given type tag.  The layout always refers to
    /// structs in terms of their defining ID (i.e. their package ID always points to the first
    /// package that introduced them).
    pub async fn type_layout(&self, mut tag: TypeTag) -> Result<MoveTypeLayout> {
        let mut context = ResolutionContext::new(self.limits.as_ref());

        // (1). Fetch all the information from this store that is necessary to resolve types
        // referenced by this tag.
        context
            .add_type_tag(
                &mut tag,
                &self.package_store,
                /* visit_fields */ true,
                /* visit_phantoms */ true,
            )
            .await?;

        // (2). Use that information to resolve the tag into a layout.
        let max_depth = self
            .limits
            .as_ref()
            .map_or(usize::MAX, |l| l.max_move_value_depth);

        Ok(context.resolve_type_layout(&tag, max_depth)?.0)
    }

    /// Return the abilities of a concrete type, based on the abilities in its type definition, and
    /// the abilities of its concrete type parameters: An instance of a generic type has `store`,
    /// `copy, or `drop` if its definition has the ability, and all its non-phantom type parameters
    /// have the ability as well. Similar rules apply for `key` except that it requires its type
    /// parameters to have `store`.
    pub async fn abilities(&self, mut tag: TypeTag) -> Result<AbilitySet> {
        let mut context = ResolutionContext::new(self.limits.as_ref());

        // (1). Fetch all the information from this store that is necessary to resolve types
        // referenced by this tag.
        context
            .add_type_tag(
                &mut tag,
                &self.package_store,
                /* visit_fields */ false,
                /* visit_phantoms */ false,
            )
            .await?;

        // (2). Use that information to calculate the type's abilities.
        context.resolve_abilities(&tag)
    }

    /// Returns the signatures of parameters to function `pkg::module::function` in the package
    /// store, assuming the function exists.
    pub async fn function_signature(
        &self,
        pkg: AccountAddress,
        module: &str,
        function: &str,
    ) -> Result<FunctionDef> {
        let mut context = ResolutionContext::new(self.limits.as_ref());

        let package = self.package_store.fetch(pkg).await?;
        let Some(mut def) = package.module(module)?.function_def(function)? else {
            return Err(Error::FunctionNotFound(
                pkg,
                module.to_string(),
                function.to_string(),
            ));
        };

        // (1). Fetch all the information from this store that is necessary to resolve types
        // referenced by this tag.
        for sig in def.parameters.iter().chain(def.return_.iter()) {
            context
                .add_signature(
                    sig.body.clone(),
                    &self.package_store,
                    package.as_ref(),
                    /* visit_fields */ false,
                )
                .await?;
        }

        // (2). Use that information to relocate package IDs in the signature.
        for sig in def.parameters.iter_mut().chain(def.return_.iter_mut()) {
            context.relocate_signature(&mut sig.body)?;
        }

        Ok(def)
    }

    /// Attempts to infer the type layouts for pure inputs to the programmable transaction.
    ///
    /// The returned vector contains an element for each input to `tx`. Elements corresponding to
    /// pure inputs that are used as arguments to transaction commands will contain `Some(layout)`.
    /// Elements for other inputs (non-pure inputs, and unused pure inputs) will be `None`.
    ///
    /// Layout resolution can fail if a type/module/package doesn't exist, if layout resolution hits
    /// a limit, or if a pure input is somehow used in multiple conflicting occasions (with
    /// different types).
    pub async fn pure_input_layouts(
        &self,
        tx: &ProgrammableTransaction,
    ) -> Result<Vec<Option<MoveTypeLayout>>> {
        let mut tags = vec![None; tx.inputs.len()];
        let mut register_type = |arg: &Argument, tag: &TypeTag| {
            let &Argument::Input(ix) = arg else {
                return Ok(());
            };

            if !matches!(tx.inputs.get(ix as usize), Some(CallArg::Pure(_))) {
                return Ok(());
            }

            let Some(type_) = tags.get_mut(ix as usize) else {
                return Ok(());
            };

            match type_ {
                None => *type_ = Some(tag.clone()),
                Some(prev) => {
                    if prev != tag {
                        return Err(Error::InputTypeConflict(ix, prev.clone(), tag.clone()));
                    }
                }
            }

            Ok(())
        };

        // (1). Infer type tags for pure inputs from their uses.
        for cmd in &tx.commands {
            match cmd {
                Command::MoveCall(call) => {
                    let params = self
                        .function_signature(
                            call.package.into(),
                            call.module.as_str(),
                            call.function.as_str(),
                        )
                        .await?
                        .parameters;

                    for (open_sig, arg) in params.iter().zip(call.arguments.iter()) {
                        let sig = open_sig.instantiate(&call.type_arguments)?;
                        register_type(arg, &sig.body)?;
                    }
                }

                Command::TransferObjects(_, arg) => register_type(arg, &TypeTag::Address)?,

                Command::SplitCoins(_, amounts) => {
                    for amount in amounts {
                        register_type(amount, &TypeTag::U64)?;
                    }
                }

                Command::MakeMoveVec(Some(tag), elems) => {
                    let tag = as_type_tag(tag)?;
                    if is_primitive_type_tag(&tag) {
                        for elem in elems {
                            register_type(elem, &tag)?;
                        }
                    }
                }

                _ => { /* nop */ }
            }
        }

        // (2). Gather all the unique type tags to convert into layouts. There are relatively few
        // primitive types so this is worth doing to avoid redundant work.
        let unique_tags: BTreeSet<_> = tags.iter().filter_map(|t| t.clone()).collect();

        // (3). Convert the type tags into layouts.
        let mut layouts = BTreeMap::new();
        for tag in unique_tags {
            let layout = self.type_layout(tag.clone()).await?;
            layouts.insert(tag, layout);
        }

        // (4) Prepare the result vector.
        Ok(tags
            .iter()
            .map(|t| t.as_ref().and_then(|t| layouts.get(t).cloned()))
            .collect())
    }

    /// Resolves a runtime address in a `ModuleId` to a storage `ModuleId` according to the linkage
    /// table in the `context` which must refer to a package.
    /// * Will fail if the wrong context is provided, i.e., is not a package, or
    ///   does not exist.
    /// * Will fail if an invalid `context` is provided for the `location`, i.e., the package at
    ///   `context` does not contain the module that `location` refers to.
    pub async fn resolve_module_id(
        &self,
        module_id: ModuleId,
        context: AccountAddress,
    ) -> Result<ModuleId> {
        let package = self.package_store.fetch(context).await?;
        let storage_id = package.relocate(*module_id.address())?;
        Ok(ModuleId::new(storage_id, module_id.name().to_owned()))
    }

    /// Resolves an abort code following the clever error format to a `CleverError` enum.
    /// The `module_id` must be the storage ID of the module (which can e.g., be gotten from the
    /// `resolve_module_id` function) and not the runtime ID.
    ///
    /// If the `abort_code` is not a clever error (i.e., does not follow the tagging and layout as
    /// defined in `ErrorBitset`), this function will return `None`.
    ///
    /// In the case where it is a clever error but only a line number is present (i.e., the error
    /// is the result of an `assert!(<cond>)` source expression) a `CleverError::LineNumberOnly` is
    /// returned. Otherwise a `CleverError::CompleteError` is returned.
    ///
    /// If for any reason we are unable to resolve the abort code to a `CleverError`, this function
    /// will return `None`.
    pub async fn resolve_clever_error(
        &self,
        module_id: ModuleId,
        abort_code: u64,
    ) -> Option<CleverError> {
        let bitset = ErrorBitset::from_u64(abort_code)?;
        let package = self.package_store.fetch(*module_id.address()).await.ok()?;
        let module = package.module(module_id.name().as_str()).ok()?.bytecode();
        let source_line_number = bitset.line_number()?;

        // We only have a line number in our clever error, so return early.
        if bitset.identifier_index().is_none() && bitset.constant_index().is_none() {
            return Some(CleverError {
                module_id,
                error_info: ErrorConstants::None,
                source_line_number,
            });
        } else if bitset.identifier_index().is_none() || bitset.constant_index().is_none() {
            return None;
        }

        let error_identifier_constant = module
            .constant_pool()
            .get(bitset.identifier_index()? as usize)?;
        let error_value_constant = module
            .constant_pool()
            .get(bitset.constant_index()? as usize)?;

        if !matches!(&error_identifier_constant.type_, SignatureToken::Vector(x) if x.as_ref() == &SignatureToken::U8)
        {
            return None;
        };

        let error_identifier = bcs::from_bytes::<Vec<u8>>(&error_identifier_constant.data)
            .ok()
            .and_then(|x| String::from_utf8(x).ok())?;
        let bytes = error_value_constant.data.clone();

        let rendered = try_render_constant(error_value_constant);

        let error_info = match rendered {
            RenderResult::NotRendered => ErrorConstants::Raw {
                identifier: error_identifier,
                bytes,
            },
            RenderResult::AsString(s) | RenderResult::AsValue(s) => ErrorConstants::Rendered {
                identifier: error_identifier,
                constant: s,
            },
        };

        Some(CleverError {
            module_id,
            error_info,
            source_line_number,
        })
    }
}

impl<T> PackageStoreWithLruCache<T> {
    pub fn new(inner: T) -> Self {
        let packages = Mutex::new(LruCache::new(PACKAGE_CACHE_SIZE));
        Self { packages, inner }
    }

    /// Removes all packages with ids in `ids` from the cache, if they exist. Does nothing for ids
    /// that are not in the cache. Accepts `self` immutably as it operates under the lock.
    pub fn evict(&self, ids: impl IntoIterator<Item = AccountAddress>) {
        let mut packages = self.packages.lock().unwrap();
        for id in ids {
            packages.pop(&id);
        }
    }
}

#[async_trait]
impl<T: PackageStore> PackageStore for PackageStoreWithLruCache<T> {
    async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>> {
        if let Some(package) = {
            // Release the lock after getting the package
            let mut packages = self.packages.lock().unwrap();
            packages.get(&id).map(Arc::clone)
        } {
            return Ok(package);
        };

        let package = self.inner.fetch(id).await?;

        // Try and insert the package into the cache, accounting for races.  In most cases the
        // racing fetches will produce the same package, but for system packages, they may not, so
        // favour the package that has the newer version, or if they are the same, the package that
        // is already in the cache.

        let mut packages = self.packages.lock().unwrap();
        Ok(match packages.peek(&id) {
            Some(prev) if package.version <= prev.version => {
                let package = prev.clone();
                packages.promote(&id);
                package
            }

            Some(_) | None => {
                packages.push(id, package.clone());
                package
            }
        })
    }
}

impl Package {
    pub fn read_from_object(object: &Object) -> Result<Self> {
        let storage_id = AccountAddress::from(object.id());
        let Some(package) = object.data.try_as_package() else {
            return Err(Error::NotAPackage(storage_id));
        };

        Self::read_from_package(package)
    }

    pub fn read_from_package(package: &MovePackage) -> Result<Self> {
        let storage_id = AccountAddress::from(package.id());
        let mut type_origins: BTreeMap<String, BTreeMap<String, AccountAddress>> = BTreeMap::new();
        for TypeOrigin {
            module_name,
            datatype_name,
            package,
        } in package.type_origin_table()
        {
            type_origins
                .entry(module_name.to_string())
                .or_default()
                .insert(datatype_name.to_string(), AccountAddress::from(*package));
        }

        let mut runtime_id = None;
        let mut modules = BTreeMap::new();
        for (name, bytes) in package.serialized_module_map() {
            let origins = type_origins.remove(name).unwrap_or_default();
            let bytecode = CompiledModule::deserialize_with_defaults(bytes)
                .map_err(|e| Error::Deserialize(e.finish(Location::Undefined)))?;

            runtime_id = Some(*bytecode.address());

            let name = name.clone();
            match Module::read(bytecode, origins) {
                Ok(module) => modules.insert(name, module),
                Err(struct_) => return Err(Error::NoTypeOrigin(storage_id, name, struct_)),
            };
        }

        let Some(runtime_id) = runtime_id else {
            return Err(Error::EmptyPackage(storage_id));
        };

        let linkage = package
            .linkage_table()
            .iter()
            .map(|(&dep, linkage)| (dep.into(), linkage.upgraded_id.into()))
            .collect();

        Ok(Package {
            storage_id,
            runtime_id,
            version: package.version(),
            modules,
            linkage,
        })
    }

    pub fn module(&self, module: &str) -> Result<&Module> {
        self.modules
            .get(module)
            .ok_or_else(|| Error::ModuleNotFound(self.storage_id, module.to_string()))
    }

    pub fn modules(&self) -> &BTreeMap<String, Module> {
        &self.modules
    }

    fn data_def(&self, module_name: &str, datatype_name: &str) -> Result<DataDef> {
        let module = self.module(module_name)?;
        let Some(data_def) = module.data_def(datatype_name)? else {
            return Err(Error::DatatypeNotFound(
                self.storage_id,
                module_name.to_string(),
                datatype_name.to_string(),
            ));
        };
        Ok(data_def)
    }

    /// Translate the `runtime_id` of a package to a specific storage ID using this package's
    /// linkage table.  Returns an error if the package in question is not present in the linkage
    /// table.
    fn relocate(&self, runtime_id: AccountAddress) -> Result<AccountAddress> {
        // Special case the current package, because it doesn't get an entry in the linkage table.
        if runtime_id == self.runtime_id {
            return Ok(self.storage_id);
        }

        self.linkage
            .get(&runtime_id)
            .ok_or_else(|| Error::LinkageNotFound(runtime_id))
            .copied()
    }
}

impl Module {
    /// Deserialize a module from its bytecode, and a table containing the origins of its structs.
    /// Fails if the origin table is missing an entry for one of its types, returning the name of
    /// the type in that case.
    fn read(
        bytecode: CompiledModule,
        mut origins: BTreeMap<String, AccountAddress>,
    ) -> std::result::Result<Self, String> {
        let mut struct_index = BTreeMap::new();
        for (index, def) in bytecode.struct_defs.iter().enumerate() {
            let sh = bytecode.datatype_handle_at(def.struct_handle);
            let struct_ = bytecode.identifier_at(sh.name).to_string();
            let index = StructDefinitionIndex::new(index as TableIndex);

            let Some(defining_id) = origins.remove(&struct_) else {
                return Err(struct_);
            };

            struct_index.insert(struct_, (defining_id, index));
        }

        let mut enum_index = BTreeMap::new();
        for (index, def) in bytecode.enum_defs.iter().enumerate() {
            let eh = bytecode.datatype_handle_at(def.enum_handle);
            let enum_ = bytecode.identifier_at(eh.name).to_string();
            let index = EnumDefinitionIndex::new(index as TableIndex);

            let Some(defining_id) = origins.remove(&enum_) else {
                return Err(enum_);
            };

            enum_index.insert(enum_, (defining_id, index));
        }

        let mut function_index = BTreeMap::new();
        for (index, def) in bytecode.function_defs.iter().enumerate() {
            let fh = bytecode.function_handle_at(def.function);
            let function = bytecode.identifier_at(fh.name).to_string();
            let index = FunctionDefinitionIndex::new(index as TableIndex);

            function_index.insert(function, index);
        }

        Ok(Module {
            bytecode,
            struct_index,
            enum_index,
            function_index,
        })
    }

    pub fn bytecode(&self) -> &CompiledModule {
        &self.bytecode
    }

    /// The module's name
    pub fn name(&self) -> &str {
        self.bytecode
            .identifier_at(self.bytecode.self_handle().name)
            .as_str()
    }

    /// Iterate over the structs with names strictly after `after` (or from the beginning), and
    /// strictly before `before` (or to the end).
    pub fn structs(
        &self,
        after: Option<&str>,
        before: Option<&str>,
    ) -> impl DoubleEndedIterator<Item = &str> + Clone {
        use std::ops::Bound as B;
        self.struct_index
            .range::<str, _>((
                after.map_or(B::Unbounded, B::Excluded),
                before.map_or(B::Unbounded, B::Excluded),
            ))
            .map(|(name, _)| name.as_str())
    }

    /// Iterate over the enums with names strictly after `after` (or from the beginning), and
    /// strictly before `before` (or to the end).
    pub fn enums(
        &self,
        after: Option<&str>,
        before: Option<&str>,
    ) -> impl DoubleEndedIterator<Item = &str> + Clone {
        use std::ops::Bound as B;
        self.enum_index
            .range::<str, _>((
                after.map_or(B::Unbounded, B::Excluded),
                before.map_or(B::Unbounded, B::Excluded),
            ))
            .map(|(name, _)| name.as_str())
    }

    /// Iterate over the datatypes with names strictly after `after` (or from the beginning), and
    /// strictly before `before` (or to the end). Enums and structs will be interleaved, and will
    /// be sorted by their names.
    pub fn datatypes(
        &self,
        after: Option<&str>,
        before: Option<&str>,
    ) -> impl DoubleEndedIterator<Item = &str> + Clone {
        let mut names = self
            .structs(after, before)
            .chain(self.enums(after, before))
            .collect::<Vec<_>>();
        names.sort();
        names.into_iter()
    }

    /// Get the struct definition corresponding to the struct with name `name` in this module.
    /// Returns `Ok(None)` if the struct cannot be found in this module, `Err(...)` if there was an
    /// error deserializing it, and `Ok(Some(def))` on success.
    pub fn struct_def(&self, name: &str) -> Result<Option<DataDef>> {
        let Some(&(defining_id, index)) = self.struct_index.get(name) else {
            return Ok(None);
        };

        let struct_def = self.bytecode.struct_def_at(index);
        let struct_handle = self.bytecode.datatype_handle_at(struct_def.struct_handle);
        let abilities = struct_handle.abilities;
        let type_params = struct_handle.type_parameters.clone();

        let fields = match &struct_def.field_information {
            StructFieldInformation::Native => vec![],
            StructFieldInformation::Declared(fields) => fields
                .iter()
                .map(|f| {
                    Ok((
                        self.bytecode.identifier_at(f.name).to_string(),
                        OpenSignatureBody::read(&f.signature.0, &self.bytecode)?,
                    ))
                })
                .collect::<Result<_>>()?,
        };

        Ok(Some(DataDef {
            defining_id,
            abilities,
            type_params,
            data: MoveData::Struct(fields),
        }))
    }

    /// Get the enum definition corresponding to the enum with name `name` in this module.
    /// Returns `Ok(None)` if the enum cannot be found in this module, `Err(...)` if there was an
    /// error deserializing it, and `Ok(Some(def))` on success.
    pub fn enum_def(&self, name: &str) -> Result<Option<DataDef>> {
        let Some(&(defining_id, index)) = self.enum_index.get(name) else {
            return Ok(None);
        };

        let enum_def = self.bytecode.enum_def_at(index);
        let enum_handle = self.bytecode.datatype_handle_at(enum_def.enum_handle);
        let abilities = enum_handle.abilities;
        let type_params = enum_handle.type_parameters.clone();

        let variants = enum_def
            .variants
            .iter()
            .map(|variant| {
                let name = self
                    .bytecode
                    .identifier_at(variant.variant_name)
                    .to_string();
                let signatures = variant
                    .fields
                    .iter()
                    .map(|f| {
                        Ok((
                            self.bytecode.identifier_at(f.name).to_string(),
                            OpenSignatureBody::read(&f.signature.0, &self.bytecode)?,
                        ))
                    })
                    .collect::<Result<_>>()?;

                Ok(VariantDef { name, signatures })
            })
            .collect::<Result<_>>()?;

        Ok(Some(DataDef {
            defining_id,
            abilities,
            type_params,
            data: MoveData::Enum(variants),
        }))
    }

    /// Get the data definition corresponding to the data type with name `name` in this module.
    /// Returns `Ok(None)` if the datatype cannot be found in this module, `Err(...)` if there was an
    /// error deserializing it, and `Ok(Some(def))` on success.
    pub fn data_def(&self, name: &str) -> Result<Option<DataDef>> {
        self.struct_def(name)
            .transpose()
            .or_else(|| self.enum_def(name).transpose())
            .transpose()
    }

    /// Iterate over the functions with names strictly after `after` (or from the beginning), and
    /// strictly before `before` (or to the end).
    pub fn functions(
        &self,
        after: Option<&str>,
        before: Option<&str>,
    ) -> impl DoubleEndedIterator<Item = &str> + Clone {
        use std::ops::Bound as B;
        self.function_index
            .range::<str, _>((
                after.map_or(B::Unbounded, B::Excluded),
                before.map_or(B::Unbounded, B::Excluded),
            ))
            .map(|(name, _)| name.as_str())
    }

    /// Get the function definition corresponding to the function with name `name` in this module.
    /// Returns `Ok(None)` if the function cannot be found in this module, `Err(...)` if there was
    /// an error deserializing it, and `Ok(Some(def))` on success.
    pub fn function_def(&self, name: &str) -> Result<Option<FunctionDef>> {
        let Some(&index) = self.function_index.get(name) else {
            return Ok(None);
        };

        let function_def = self.bytecode.function_def_at(index);
        let function_handle = self.bytecode.function_handle_at(function_def.function);

        Ok(Some(FunctionDef {
            visibility: function_def.visibility,
            is_entry: function_def.is_entry,
            type_params: function_handle.type_parameters.clone(),
            parameters: read_signature(function_handle.parameters, &self.bytecode)?,
            return_: read_signature(function_handle.return_, &self.bytecode)?,
        }))
    }
}

impl OpenSignature {
    fn read(sig: &SignatureToken, bytecode: &CompiledModule) -> Result<Self> {
        use SignatureToken as S;
        Ok(match sig {
            S::Reference(sig) => OpenSignature {
                ref_: Some(Reference::Immutable),
                body: OpenSignatureBody::read(sig, bytecode)?,
            },

            S::MutableReference(sig) => OpenSignature {
                ref_: Some(Reference::Mutable),
                body: OpenSignatureBody::read(sig, bytecode)?,
            },

            sig => OpenSignature {
                ref_: None,
                body: OpenSignatureBody::read(sig, bytecode)?,
            },
        })
    }

    /// Return a specific instantiation of this signature, with `type_params` as the actual type
    /// parameters. This function does not check that the supplied type parameters are valid (meet
    /// the ability constraints of the struct or function this signature is part of), but will
    /// produce an error if the signature references a type parameter that is out of bounds.
    pub fn instantiate(&self, type_params: &[TypeInput]) -> Result<Signature> {
        Ok(Signature {
            ref_: self.ref_,
            body: self.body.instantiate(type_params)?,
        })
    }
}

impl OpenSignatureBody {
    fn read(sig: &SignatureToken, bytecode: &CompiledModule) -> Result<Self> {
        use OpenSignatureBody as O;
        use SignatureToken as S;

        Ok(match sig {
            S::Signer => return Err(Error::UnexpectedSigner),
            S::Reference(_) | S::MutableReference(_) => return Err(Error::UnexpectedReference),

            S::Address => O::Address,
            S::Bool => O::Bool,
            S::U8 => O::U8,
            S::U16 => O::U16,
            S::U32 => O::U32,
            S::U64 => O::U64,
            S::U128 => O::U128,
            S::U256 => O::U256,
            S::TypeParameter(ix) => O::TypeParameter(*ix),

            S::Vector(sig) => O::Vector(Box::new(OpenSignatureBody::read(sig, bytecode)?)),

            S::Datatype(ix) => O::Datatype(DatatypeKey::read(*ix, bytecode), vec![]),
            S::DatatypeInstantiation(inst) => {
                let (ix, params) = &**inst;
                O::Datatype(
                    DatatypeKey::read(*ix, bytecode),
                    params
                        .iter()
                        .map(|sig| OpenSignatureBody::read(sig, bytecode))
                        .collect::<Result<_>>()?,
                )
            }
        })
    }

    fn instantiate(&self, type_params: &[TypeInput]) -> Result<TypeTag> {
        use OpenSignatureBody as O;
        use TypeTag as T;

        Ok(match self {
            O::Address => T::Address,
            O::Bool => T::Bool,
            O::U8 => T::U8,
            O::U16 => T::U16,
            O::U32 => T::U32,
            O::U64 => T::U64,
            O::U128 => T::U128,
            O::U256 => T::U256,
            O::Vector(s) => T::Vector(Box::new(s.instantiate(type_params)?)),

            O::Datatype(key, dty_params) => T::Struct(Box::new(StructTag {
                address: key.package,
                module: ident(&key.module)?,
                name: ident(&key.name)?,
                type_params: dty_params
                    .iter()
                    .map(|p| p.instantiate(type_params))
                    .collect::<Result<_>>()?,
            })),

            O::TypeParameter(ix) => as_type_tag(
                type_params
                    .get(*ix as usize)
                    .ok_or_else(|| Error::TypeParamOOB(*ix, type_params.len()))?,
            )?,
        })
    }
}

impl<'m, 'n> DatatypeRef<'m, 'n> {
    pub fn as_key(&self) -> DatatypeKey {
        DatatypeKey {
            package: self.package,
            module: self.module.to_string().into(),
            name: self.name.to_string().into(),
        }
    }
}

impl DatatypeKey {
    fn read(ix: DatatypeHandleIndex, bytecode: &CompiledModule) -> Self {
        let sh = bytecode.datatype_handle_at(ix);
        let mh = bytecode.module_handle_at(sh.module);

        let package = *bytecode.address_identifier_at(mh.address);
        let module = bytecode.identifier_at(mh.name).to_string().into();
        let name = bytecode.identifier_at(sh.name).to_string().into();

        DatatypeKey {
            package,
            module,
            name,
        }
    }
}

impl<'l> ResolutionContext<'l> {
    fn new(limits: Option<&'l Limits>) -> Self {
        ResolutionContext {
            datatypes: BTreeMap::new(),
            limits,
        }
    }

    /// Gather definitions for types that contribute to the definition of `tag` into this resolution
    /// context, fetching data from the `store` as necessary. Also updates package addresses in
    /// `tag` to point to runtime IDs instead of storage IDs to ensure queries made using these
    /// addresses during the subsequent resolution phase find the relevant type information in the
    /// context.
    ///
    /// The `visit_fields` flag controls whether the traversal looks inside types at their fields
    /// (which is necessary for layout resolution) or not (only explores the outer type and any type
    /// parameters).
    ///
    /// The `visit_phantoms` flag controls whether the traversal recurses through phantom type
    /// parameters (which is also necessary for type resolution) or not.
    async fn add_type_tag<S: PackageStore + ?Sized>(
        &mut self,
        tag: &mut TypeTag,
        store: &S,
        visit_fields: bool,
        visit_phantoms: bool,
    ) -> Result<()> {
        use TypeTag as T;

        struct ToVisit<'t> {
            tag: &'t mut TypeTag,
            depth: usize,
        }

        let mut frontier = vec![ToVisit { tag, depth: 0 }];
        while let Some(ToVisit { tag, depth }) = frontier.pop() {
            macro_rules! push_ty_param {
                ($tag:expr) => {{
                    check_max_limit!(
                        TypeParamNesting, self.limits;
                        max_type_argument_depth > depth
                    );

                    frontier.push(ToVisit { tag: $tag, depth: depth + 1 })
                }}
            }

            match tag {
                T::Address
                | T::Bool
                | T::U8
                | T::U16
                | T::U32
                | T::U64
                | T::U128
                | T::U256
                | T::Signer => {
                    // Nothing further to add to context
                }

                T::Vector(tag) => push_ty_param!(tag),

                T::Struct(s) => {
                    let context = store.fetch(s.address).await?;
                    let def = context
                        .clone()
                        .data_def(s.module.as_str(), s.name.as_str())?;

                    // Normalize `address` (the ID of a package that contains the definition of this
                    // struct) to be a runtime ID, because that's what the resolution context uses
                    // for keys.  Take care to do this before generating the key that is used to
                    // query and/or write into `self.structs.
                    s.address = context.runtime_id;
                    let key = DatatypeRef::from(s.as_ref()).as_key();

                    if def.type_params.len() != s.type_params.len() {
                        return Err(Error::TypeArityMismatch(
                            def.type_params.len(),
                            s.type_params.len(),
                        ));
                    }

                    check_max_limit!(
                        TooManyTypeParams, self.limits;
                        max_type_argument_width >= s.type_params.len()
                    );

                    for (param, def) in s.type_params.iter_mut().zip(def.type_params.iter()) {
                        if !def.is_phantom || visit_phantoms {
                            push_ty_param!(param);
                        }
                    }

                    if self.datatypes.contains_key(&key) {
                        continue;
                    }

                    if visit_fields {
                        match &def.data {
                            MoveData::Struct(fields) => {
                                for (_, sig) in fields {
                                    self.add_signature(sig.clone(), store, &context, visit_fields)
                                        .await?;
                                }
                            }
                            MoveData::Enum(variants) => {
                                for variant in variants {
                                    for (_, sig) in &variant.signatures {
                                        self.add_signature(
                                            sig.clone(),
                                            store,
                                            &context,
                                            visit_fields,
                                        )
                                        .await?;
                                    }
                                }
                            }
                        };
                    }

                    check_max_limit!(
                        TooManyTypeNodes, self.limits;
                        max_type_nodes > self.datatypes.len()
                    );

                    self.datatypes.insert(key, def);
                }
            }
        }

        Ok(())
    }

    // Like `add_type_tag` but for type signatures.  Needs a linkage table to translate runtime IDs
    // into storage IDs.
    async fn add_signature<T: PackageStore + ?Sized>(
        &mut self,
        sig: OpenSignatureBody,
        store: &T,
        context: &Package,
        visit_fields: bool,
    ) -> Result<()> {
        use OpenSignatureBody as O;

        let mut frontier = vec![sig];
        while let Some(sig) = frontier.pop() {
            match sig {
                O::Address
                | O::Bool
                | O::U8
                | O::U16
                | O::U32
                | O::U64
                | O::U128
                | O::U256
                | O::TypeParameter(_) => {
                    // Nothing further to add to context
                }

                O::Vector(sig) => frontier.push(*sig),

                O::Datatype(key, params) => {
                    check_max_limit!(
                        TooManyTypeParams, self.limits;
                        max_type_argument_width >= params.len()
                    );

                    let params_count = params.len();
                    let data_count = self.datatypes.len();
                    frontier.extend(params.into_iter());

                    let type_params = if let Some(def) = self.datatypes.get(&key) {
                        &def.type_params
                    } else {
                        check_max_limit!(
                            TooManyTypeNodes, self.limits;
                            max_type_nodes > data_count
                        );

                        // Need to resolve the datatype, so fetch the package that contains it.
                        let storage_id = context.relocate(key.package)?;
                        let package = store.fetch(storage_id).await?;

                        let def = package.data_def(&key.module, &key.name)?;
                        if visit_fields {
                            match &def.data {
                                MoveData::Struct(fields) => {
                                    frontier.extend(fields.iter().map(|f| &f.1).cloned());
                                }
                                MoveData::Enum(variants) => {
                                    frontier.extend(
                                        variants
                                            .iter()
                                            .flat_map(|v| v.signatures.iter().map(|(_, s)| s))
                                            .cloned(),
                                    );
                                }
                            };
                        }

                        &self.datatypes.entry(key).or_insert(def).type_params
                    };

                    if type_params.len() != params_count {
                        return Err(Error::TypeArityMismatch(type_params.len(), params_count));
                    }
                }
            }
        }

        Ok(())
    }

    /// Translate runtime IDs in a type `tag` into defining IDs using only the information
    /// contained in this context. Requires that the necessary information was added to the context
    /// through calls to `add_type_tag`.
    fn canonicalize_type(&self, tag: &mut TypeTag) -> Result<()> {
        use TypeTag as T;

        match tag {
            T::Signer => return Err(Error::UnexpectedSigner),
            T::Address | T::Bool | T::U8 | T::U16 | T::U32 | T::U64 | T::U128 | T::U256 => {
                /* nop */
            }

            T::Vector(tag) => self.canonicalize_type(tag.as_mut())?,

            T::Struct(s) => {
                for tag in &mut s.type_params {
                    self.canonicalize_type(tag)?;
                }

                // SAFETY: `add_type_tag` ensures `datatyps` has an element with this key.
                let key = DatatypeRef::from(s.as_ref());
                let def = &self.datatypes[&key];

                s.address = def.defining_id;
            }
        }

        Ok(())
    }

    /// Translate a type `tag` into its layout using only the information contained in this context.
    /// Requires that the necessary information was added to the context through calls to
    /// `add_type_tag` and `add_signature` before being called.
    ///
    /// `max_depth` controls how deep the layout is allowed to grow to. The actual depth reached is
    /// returned alongside the layout (assuming it does not exceed `max_depth`).
    fn resolve_type_layout(
        &self,
        tag: &TypeTag,
        max_depth: usize,
    ) -> Result<(MoveTypeLayout, usize)> {
        use MoveTypeLayout as L;
        use TypeTag as T;

        if max_depth == 0 {
            return Err(Error::ValueNesting(
                self.limits.map_or(0, |l| l.max_move_value_depth),
            ));
        }

        Ok(match tag {
            T::Signer => return Err(Error::UnexpectedSigner),

            T::Address => (L::Address, 1),
            T::Bool => (L::Bool, 1),
            T::U8 => (L::U8, 1),
            T::U16 => (L::U16, 1),
            T::U32 => (L::U32, 1),
            T::U64 => (L::U64, 1),
            T::U128 => (L::U128, 1),
            T::U256 => (L::U256, 1),

            T::Vector(tag) => {
                let (layout, depth) = self.resolve_type_layout(tag, max_depth - 1)?;
                (L::Vector(Box::new(layout)), depth + 1)
            }

            T::Struct(s) => {
                // TODO (optimization): Could introduce a layout cache to further speed up
                // resolution.  Relevant entries in that cache would need to be gathered in the
                // ResolutionContext as it is built, and then used here to avoid the recursive
                // exploration.  This optimisation is complicated by the fact that in the cache,
                // these layouts are naturally keyed based on defining ID, but during resolution,
                // they are keyed by runtime IDs.

                // TODO (optimization): This could be made more efficient by only generating layouts
                // for non-phantom types.  This efficiency could be extended to the exploration
                // phase (i.e. only explore layouts of non-phantom types). But this optimisation is
                // complicated by the fact that we still need to create a correct type tag for a
                // phantom parameter, which is currently done by converting a type layout into a
                // tag.
                let param_layouts = s
                    .type_params
                    .iter()
                    // Reduce the max depth because we know these type parameters will be nested
                    // within this struct.
                    .map(|tag| self.resolve_type_layout(tag, max_depth - 1))
                    .collect::<Result<Vec<_>>>()?;

                // SAFETY: `param_layouts` contains `MoveTypeLayout`-s that are generated by this
                // `ResolutionContext`, which guarantees that struct layouts come with types, which
                // is necessary to avoid errors when converting layouts into type tags.
                let type_params = param_layouts.iter().map(|l| TypeTag::from(&l.0)).collect();

                // SAFETY: `add_type_tag` ensures `datatyps` has an element with this key.
                let key = DatatypeRef::from(s.as_ref());
                let def = &self.datatypes[&key];

                let type_ = StructTag {
                    address: def.defining_id,
                    module: s.module.clone(),
                    name: s.name.clone(),
                    type_params,
                };

                self.resolve_datatype_signature(def, type_, param_layouts, max_depth)?
            }
        })
    }

    /// Translates a datatype definition into a type layout.  Needs to be provided the layouts of type
    /// parameters which are substituted when a type parameter is encountered.
    ///
    /// `max_depth` controls how deep the layout is allowed to grow to. The actual depth reached is
    /// returned alongside the layout (assuming it does not exceed `max_depth`).
    fn resolve_datatype_signature(
        &self,
        data_def: &DataDef,
        type_: StructTag,
        param_layouts: Vec<(MoveTypeLayout, usize)>,
        max_depth: usize,
    ) -> Result<(MoveTypeLayout, usize)> {
        Ok(match &data_def.data {
            MoveData::Struct(fields) => {
                let mut resolved_fields = Vec::with_capacity(fields.len());
                let mut field_depth = 0;

                for (name, sig) in fields {
                    let (layout, depth) =
                        self.resolve_signature_layout(sig, &param_layouts, max_depth - 1)?;

                    field_depth = field_depth.max(depth);
                    resolved_fields.push(MoveFieldLayout {
                        name: ident(name.as_str())?,
                        layout,
                    })
                }

                (
                    MoveTypeLayout::Struct(Box::new(MoveStructLayout {
                        type_,
                        fields: resolved_fields,
                    })),
                    field_depth + 1,
                )
            }
            MoveData::Enum(variants) => {
                let mut field_depth = 0;
                let mut resolved_variants = BTreeMap::new();

                for (tag, variant) in variants.iter().enumerate() {
                    let mut fields = Vec::with_capacity(variant.signatures.len());
                    for (name, sig) in &variant.signatures {
                        // Note: We decrement the depth here because we're already under the variant
                        let (layout, depth) =
                            self.resolve_signature_layout(sig, &param_layouts, max_depth - 1)?;

                        field_depth = field_depth.max(depth);
                        fields.push(MoveFieldLayout {
                            name: ident(name.as_str())?,
                            layout,
                        })
                    }
                    resolved_variants.insert((ident(variant.name.as_str())?, tag as u16), fields);
                }

                (
                    MoveTypeLayout::Enum(Box::new(MoveEnumLayout {
                        type_,
                        variants: resolved_variants,
                    })),
                    field_depth + 1,
                )
            }
        })
    }

    /// Like `resolve_type_tag` but for signatures.  Needs to be provided the layouts of type
    /// parameters which are substituted when a type parameter is encountered.
    ///
    /// `max_depth` controls how deep the layout is allowed to grow to. The actual depth reached is
    /// returned alongside the layout (assuming it does not exceed `max_depth`).
    fn resolve_signature_layout(
        &self,
        sig: &OpenSignatureBody,
        param_layouts: &[(MoveTypeLayout, usize)],
        max_depth: usize,
    ) -> Result<(MoveTypeLayout, usize)> {
        use MoveTypeLayout as L;
        use OpenSignatureBody as O;

        if max_depth == 0 {
            return Err(Error::ValueNesting(
                self.limits.map_or(0, |l| l.max_move_value_depth),
            ));
        }

        Ok(match sig {
            O::Address => (L::Address, 1),
            O::Bool => (L::Bool, 1),
            O::U8 => (L::U8, 1),
            O::U16 => (L::U16, 1),
            O::U32 => (L::U32, 1),
            O::U64 => (L::U64, 1),
            O::U128 => (L::U128, 1),
            O::U256 => (L::U256, 1),

            O::TypeParameter(ix) => {
                let (layout, depth) = param_layouts
                    .get(*ix as usize)
                    .ok_or_else(|| Error::TypeParamOOB(*ix, param_layouts.len()))
                    .cloned()?;

                // We need to re-check the type parameter before we use it because it might have
                // been fine when it was created, but result in too deep a layout when we use it at
                // this position.
                if depth > max_depth {
                    return Err(Error::ValueNesting(
                        self.limits.map_or(0, |l| l.max_move_value_depth),
                    ));
                }

                (layout, depth)
            }

            O::Vector(sig) => {
                let (layout, depth) =
                    self.resolve_signature_layout(sig.as_ref(), param_layouts, max_depth - 1)?;

                (L::Vector(Box::new(layout)), depth + 1)
            }

            O::Datatype(key, params) => {
                // SAFETY: `add_signature` ensures `datatypes` has an element with this key.
                let def = &self.datatypes[key];

                let param_layouts = params
                    .iter()
                    .map(|sig| self.resolve_signature_layout(sig, param_layouts, max_depth - 1))
                    .collect::<Result<Vec<_>>>()?;

                // SAFETY: `param_layouts` contains `MoveTypeLayout`-s that are generated by this
                // `ResolutionContext`, which guarantees that struct layouts come with types, which
                // is necessary to avoid errors when converting layouts into type tags.
                let type_params: Vec<TypeTag> =
                    param_layouts.iter().map(|l| TypeTag::from(&l.0)).collect();

                let type_ = StructTag {
                    address: def.defining_id,
                    module: ident(&key.module)?,
                    name: ident(&key.name)?,
                    type_params,
                };

                self.resolve_datatype_signature(def, type_, param_layouts, max_depth)?
            }
        })
    }

    /// Calculate the abilities for a concrete type `tag`. Requires that the necessary information
    /// was added to the context through calls to `add_type_tag` before being called.
    fn resolve_abilities(&self, tag: &TypeTag) -> Result<AbilitySet> {
        use TypeTag as T;
        Ok(match tag {
            T::Signer => return Err(Error::UnexpectedSigner),

            T::Bool | T::U8 | T::U16 | T::U32 | T::U64 | T::U128 | T::U256 | T::Address => {
                AbilitySet::PRIMITIVES
            }

            T::Vector(tag) => self.resolve_abilities(tag)?.intersect(AbilitySet::VECTOR),

            T::Struct(s) => {
                // SAFETY: `add_type_tag` ensures `datatypes` has an element with this key.
                let key = DatatypeRef::from(s.as_ref());
                let def = &self.datatypes[&key];

                if def.type_params.len() != s.type_params.len() {
                    return Err(Error::TypeArityMismatch(
                        def.type_params.len(),
                        s.type_params.len(),
                    ));
                }

                let param_abilities: Result<Vec<AbilitySet>> = s
                    .type_params
                    .iter()
                    .zip(def.type_params.iter())
                    .map(|(p, d)| {
                        if d.is_phantom {
                            Ok(AbilitySet::EMPTY)
                        } else {
                            self.resolve_abilities(p)
                        }
                    })
                    .collect();

                AbilitySet::polymorphic_abilities(
                    def.abilities,
                    def.type_params.iter().map(|p| p.is_phantom),
                    param_abilities?.into_iter(),
                )
                // This error is unexpected because the only reason it would fail is because of a
                // type parameter arity mismatch, which we check for above.
                .map_err(|e| Error::UnexpectedError(Arc::new(e)))?
            }
        })
    }

    /// Translate the (runtime) package IDs in `sig` to defining IDs using only the information
    /// contained in this context. Requires that the necessary information was added to the context
    /// through calls to `add_signature` before being called.
    fn relocate_signature(&self, sig: &mut OpenSignatureBody) -> Result<()> {
        use OpenSignatureBody as O;

        match sig {
            O::Address | O::Bool | O::U8 | O::U16 | O::U32 | O::U64 | O::U128 | O::U256 => {
                /* nop */
            }

            O::TypeParameter(_) => { /* nop */ }

            O::Vector(sig) => self.relocate_signature(sig.as_mut())?,

            O::Datatype(key, params) => {
                // SAFETY: `add_signature` ensures `datatypes` has an element with this key.
                let defining_id = &self.datatypes[key].defining_id;
                for param in params {
                    self.relocate_signature(param)?;
                }

                key.package = *defining_id;
            }
        }

        Ok(())
    }
}

impl<'s> From<&'s StructTag> for DatatypeRef<'s, 's> {
    fn from(tag: &'s StructTag) -> Self {
        DatatypeRef {
            package: tag.address,
            module: tag.module.as_str().into(),
            name: tag.name.as_str().into(),
        }
    }
}

/// Translate a string into an `Identifier`, but translating errors into this module's error type.
fn ident(s: &str) -> Result<Identifier> {
    Identifier::new(s).map_err(|_| Error::NotAnIdentifier(s.to_string()))
}

pub fn as_type_tag(type_input: &TypeInput) -> Result<TypeTag> {
    use TypeInput as I;
    use TypeTag as T;
    Ok(match type_input {
        I::Bool => T::Bool,
        I::U8 => T::U8,
        I::U16 => T::U16,
        I::U32 => T::U32,
        I::U64 => T::U64,
        I::U128 => T::U128,
        I::U256 => T::U256,
        I::Address => T::Address,
        I::Signer => T::Signer,
        I::Vector(t) => T::Vector(Box::new(as_type_tag(t)?)),
        I::Struct(s) => {
            let StructInput {
                address,
                module,
                name,
                type_params,
            } = s.as_ref();
            let type_params = type_params.iter().map(as_type_tag).collect::<Result<_>>()?;
            T::Struct(Box::new(StructTag {
                address: *address,
                module: ident(module)?,
                name: ident(name)?,
                type_params,
            }))
        }
    })
}

/// Read and deserialize a signature index (from function parameter or return types) into a vector
/// of signatures.
fn read_signature(idx: SignatureIndex, bytecode: &CompiledModule) -> Result<Vec<OpenSignature>> {
    let MoveSignature(tokens) = bytecode.signature_at(idx);
    let mut sigs = Vec::with_capacity(tokens.len());

    for token in tokens {
        sigs.push(OpenSignature::read(token, bytecode)?);
    }

    Ok(sigs)
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use move_binary_format::file_format::Ability;
    use move_core_types::ident_str;
    use std::sync::Arc;
    use std::{path::PathBuf, str::FromStr, sync::RwLock};
    use sui_types::base_types::random_object_ref;
    use sui_types::transaction::ObjectArg;

    use move_compiler::compiled_unit::NamedCompiledModule;
    use sui_move_build::{BuildConfig, CompiledPackage};

    use super::*;

    fn fmt(struct_layout: MoveTypeLayout, enum_layout: MoveTypeLayout) -> String {
        format!("struct:\n{struct_layout:#}\n\nenum:\n{enum_layout:#}",)
    }

    #[tokio::test]
    async fn test_simple_canonical_type() {
        let (_, cache) = package_cache([(1, build_package("a0"), a0_types())]);
        let package_resolver = Resolver::new(cache);

        let input = type_("0xa0::m::T0");
        let expect = input.clone();
        let actual = package_resolver.canonical_type(input).await.unwrap();
        assert_eq!(expect, actual);
    }

    #[tokio::test]
    async fn test_upgraded_canonical_type() {
        let (_, cache) = package_cache([
            (1, build_package("a0"), a0_types()),
            (2, build_package("a1"), a1_types()),
        ]);

        let package_resolver = Resolver::new(cache);

        let input = type_("0xa1::m::T3");
        let expect = input.clone();
        let actual = package_resolver.canonical_type(input).await.unwrap();
        assert_eq!(expect, actual);
    }

    #[tokio::test]
    async fn test_latest_canonical_type() {
        let (_, cache) = package_cache([
            (1, build_package("a0"), a0_types()),
            (2, build_package("a1"), a1_types()),
        ]);

        let package_resolver = Resolver::new(cache);

        let input = type_("0xa1::m::T0");
        let expect = type_("0xa0::m::T0");
        let actual = package_resolver.canonical_type(input).await.unwrap();
        assert_eq!(expect, actual);
    }

    #[tokio::test]
    async fn test_type_param_canonical_type() {
        let (_, cache) = package_cache([
            (1, build_package("a0"), a0_types()),
            (2, build_package("a1"), a1_types()),
        ]);

        let package_resolver = Resolver::new(cache);

        let input = type_("0xa1::m::T1<0xa1::m::T0, 0xa1::m::T3>");
        let expect = type_("0xa0::m::T1<0xa0::m::T0, 0xa1::m::T3>");
        let actual = package_resolver.canonical_type(input).await.unwrap();
        assert_eq!(expect, actual);
    }

    #[tokio::test]
    async fn test_canonical_err_package_too_old() {
        let (_, cache) = package_cache([
            (1, build_package("a0"), a0_types()),
            (2, build_package("a1"), a1_types()),
        ]);

        let package_resolver = Resolver::new(cache);

        let input = type_("0xa0::m::T3");
        let err = package_resolver.canonical_type(input).await.unwrap_err();
        assert!(matches!(err, Error::DatatypeNotFound(_, _, _)));
    }

    #[tokio::test]
    async fn test_canonical_err_signer() {
        let (_, cache) = package_cache([(1, build_package("a0"), a0_types())]);

        let package_resolver = Resolver::new(cache);

        let input = type_("0xa0::m::T1<0xa0::m::T0, signer>");
        let err = package_resolver.canonical_type(input).await.unwrap_err();
        assert!(matches!(err, Error::UnexpectedSigner));
    }

    /// Layout for a type that only refers to base types or other types in the same module.
    #[tokio::test]
    async fn test_simple_type_layout() {
        let (_, cache) = package_cache([(1, build_package("a0"), a0_types())]);
        let package_resolver = Resolver::new(cache);
        let struct_layout = package_resolver
            .type_layout(type_("0xa0::m::T0"))
            .await
            .unwrap();
        let enum_layout = package_resolver
            .type_layout(type_("0xa0::m::E0"))
            .await
            .unwrap();
        insta::assert_snapshot!(fmt(struct_layout, enum_layout));
    }

    /// A type that refers to types from other modules in the same package.
    #[tokio::test]
    async fn test_cross_module_layout() {
        let (_, cache) = package_cache([(1, build_package("a0"), a0_types())]);
        let resolver = Resolver::new(cache);
        let struct_layout = resolver.type_layout(type_("0xa0::n::T0")).await.unwrap();
        let enum_layout = resolver.type_layout(type_("0xa0::n::E0")).await.unwrap();
        insta::assert_snapshot!(fmt(struct_layout, enum_layout));
    }

    /// A type that refers to types a different package.
    #[tokio::test]
    async fn test_cross_package_layout() {
        let (_, cache) = package_cache([
            (1, build_package("a0"), a0_types()),
            (1, build_package("b0"), b0_types()),
        ]);
        let resolver = Resolver::new(cache);

        let struct_layout = resolver.type_layout(type_("0xb0::m::T0")).await.unwrap();
        let enum_layout = resolver.type_layout(type_("0xb0::m::E0")).await.unwrap();
        insta::assert_snapshot!(fmt(struct_layout, enum_layout));
    }

    /// A type from an upgraded package, mixing structs defined in the original package and the
    /// upgraded package.
    #[tokio::test]
    async fn test_upgraded_package_layout() {
        let (_, cache) = package_cache([
            (1, build_package("a0"), a0_types()),
            (2, build_package("a1"), a1_types()),
        ]);
        let resolver = Resolver::new(cache);

        let struct_layout = resolver.type_layout(type_("0xa1::n::T1")).await.unwrap();
        let enum_layout = resolver.type_layout(type_("0xa1::n::E1")).await.unwrap();
        insta::assert_snapshot!(fmt(struct_layout, enum_layout));
    }

    /// A generic type instantiation where the type parameters are resolved relative to linkage
    /// contexts from different versions of the same package.
    #[tokio::test]
    async fn test_multiple_linkage_contexts_layout() {
        let (_, cache) = package_cache([
            (1, build_package("a0"), a0_types()),
            (2, build_package("a1"), a1_types()),
        ]);
        let resolver = Resolver::new(cache);

        let struct_layout = resolver
            .type_layout(type_("0xa0::m::T1<0xa0::m::T0, 0xa1::m::T3>"))
            .await
            .unwrap();
        let enum_layout = resolver
            .type_layout(type_("0xa0::m::E1<0xa0::m::E0, 0xa1::m::E3>"))
            .await
            .unwrap();
        insta::assert_snapshot!(fmt(struct_layout, enum_layout));
    }

    /// Refer to a type, not by its defining ID, but by the ID of some later version of that
    /// package.  This doesn't currently work during execution but it simplifies making queries: A
    /// type can be referred to using the ID of any package that declares it, rather than only the
    /// package that first declared it (whose ID is its defining ID).
    #[tokio::test]
    async fn test_upgraded_package_non_defining_id_layout() {
        let (_, cache) = package_cache([
            (1, build_package("a0"), a0_types()),
            (2, build_package("a1"), a1_types()),
        ]);
        let resolver = Resolver::new(cache);

        let struct_layout = resolver
            .type_layout(type_("0xa1::m::T1<0xa1::m::T3, 0xa1::m::T0>"))
            .await
            .unwrap();
        let enum_layout = resolver
            .type_layout(type_("0xa1::m::E1<0xa1::m::E3, 0xa1::m::E0>"))
            .await
            .unwrap();
        insta::assert_snapshot!(fmt(struct_layout, enum_layout));
    }

    /// A type that refers to a types in a relinked package.  C depends on B and overrides its
    /// dependency on A from v1 to v2.  The type in C refers to types that were defined in both B, A
    /// v1, and A v2.
    #[tokio::test]
    async fn test_relinking_layout() {
        let (_, cache) = package_cache([
            (1, build_package("a0"), a0_types()),
            (2, build_package("a1"), a1_types()),
            (1, build_package("b0"), b0_types()),
            (1, build_package("c0"), c0_types()),
        ]);
        let resolver = Resolver::new(cache);

        let struct_layout = resolver.type_layout(type_("0xc0::m::T0")).await.unwrap();
        let enum_layout = resolver.type_layout(type_("0xc0::m::E0")).await.unwrap();
        insta::assert_snapshot!(fmt(struct_layout, enum_layout));
    }

    #[tokio::test]
    async fn test_value_nesting_boundary_layout() {
        let (_, cache) = package_cache([(1, build_package("a0"), a0_types())]);

        let resolver = Resolver::new_with_limits(
            cache,
            Limits {
                max_type_argument_width: 100,
                max_type_argument_depth: 100,
                max_type_nodes: 100,
                max_move_value_depth: 3,
            },
        );

        // The layout of this type is fine, because it is *just* at the correct depth.
        let struct_layout = resolver
            .type_layout(type_("0xa0::m::T1<u8, u8>"))
            .await
            .unwrap();
        let enum_layout = resolver
            .type_layout(type_("0xa0::m::E1<u8, u8>"))
            .await
            .unwrap();
        insta::assert_snapshot!(fmt(struct_layout, enum_layout));
    }

    #[tokio::test]
    async fn test_err_value_nesting_simple_layout() {
        let (_, cache) = package_cache([(1, build_package("a0"), a0_types())]);

        let resolver = Resolver::new_with_limits(
            cache,
            Limits {
                max_type_argument_width: 100,
                max_type_argument_depth: 100,
                max_type_nodes: 100,
                max_move_value_depth: 2,
            },
        );

        // The depth limit is now too low, so this will fail.
        let struct_err = resolver
            .type_layout(type_("0xa0::m::T1<u8, u8>"))
            .await
            .unwrap_err();
        let enum_err = resolver
            .type_layout(type_("0xa0::m::E1<u8, u8>"))
            .await
            .unwrap_err();
        assert!(matches!(struct_err, Error::ValueNesting(2)));
        assert!(matches!(enum_err, Error::ValueNesting(2)));
    }

    #[tokio::test]
    async fn test_err_value_nesting_big_type_param_layout() {
        let (_, cache) = package_cache([(1, build_package("a0"), a0_types())]);

        let resolver = Resolver::new_with_limits(
            cache,
            Limits {
                max_type_argument_width: 100,
                max_type_argument_depth: 100,
                max_type_nodes: 100,
                max_move_value_depth: 3,
            },
        );

        // This layout calculation will fail early because we know that the type parameter we're
        // calculating will eventually contribute to a layout that exceeds the max depth.
        let struct_err = resolver
            .type_layout(type_("0xa0::m::T1<vector<vector<u8>>, u8>"))
            .await
            .unwrap_err();
        let enum_err = resolver
            .type_layout(type_("0xa0::m::E1<vector<vector<u8>>, u8>"))
            .await
            .unwrap_err();
        assert!(matches!(struct_err, Error::ValueNesting(3)));
        assert!(matches!(enum_err, Error::ValueNesting(3)));
    }

    #[tokio::test]
    async fn test_err_value_nesting_big_phantom_type_param_layout() {
        let (_, cache) = package_cache([
            (1, build_package("sui"), sui_types()),
            (1, build_package("d0"), d0_types()),
        ]);

        let resolver = Resolver::new_with_limits(
            cache,
            Limits {
                max_type_argument_width: 100,
                max_type_argument_depth: 100,
                max_type_nodes: 100,
                max_move_value_depth: 3,
            },
        );

        // Check that this layout request would succeed.
        let _ = resolver
            .type_layout(type_("0xd0::m::O<u8, u8>"))
            .await
            .unwrap();
        let _ = resolver
            .type_layout(type_("0xd0::m::EO<u8, u8>"))
            .await
            .unwrap();

        // But this one fails, even though the big layout is for a phantom type parameter. This may
        // change in future if we optimise the way we handle phantom type parameters to not
        // calculate their full layout, just their type tag.
        let struct_err = resolver
            .type_layout(type_("0xd0::m::O<u8, vector<vector<u8>>>"))
            .await
            .unwrap_err();
        let enum_err = resolver
            .type_layout(type_("0xd0::m::EO<u8, vector<vector<u8>>>"))
            .await
            .unwrap_err();
        assert!(matches!(struct_err, Error::ValueNesting(3)));
        assert!(matches!(enum_err, Error::ValueNesting(3)));
    }

    #[tokio::test]
    async fn test_err_value_nesting_type_param_application_layout() {
        let (_, cache) = package_cache([
            (1, build_package("sui"), sui_types()),
            (1, build_package("d0"), d0_types()),
        ]);

        let resolver = Resolver::new_with_limits(
            cache,
            Limits {
                max_type_argument_width: 100,
                max_type_argument_depth: 100,
                max_type_nodes: 100,
                max_move_value_depth: 3,
            },
        );

        // Make sure that even if all type parameters individually meet the depth requirements,
        // that we correctly fail if they extend the layout's depth on application.
        let struct_err = resolver
            .type_layout(type_("0xd0::m::O<vector<u8>, u8>"))
            .await
            .unwrap_err();
        let enum_err = resolver
            .type_layout(type_("0xd0::m::EO<vector<u8>, u8>"))
            .await
            .unwrap_err();

        assert!(matches!(struct_err, Error::ValueNesting(3)));
        assert!(matches!(enum_err, Error::ValueNesting(3)));
    }

    #[tokio::test]
    async fn test_system_package_invalidation() {
        let (inner, cache) = package_cache([(1, build_package("s0"), s0_types())]);
        let resolver = Resolver::new(cache);

        let struct_not_found = resolver.type_layout(type_("0x1::m::T1")).await.unwrap_err();
        let enum_not_found = resolver.type_layout(type_("0x1::m::E1")).await.unwrap_err();
        assert!(matches!(struct_not_found, Error::DatatypeNotFound(_, _, _)));
        assert!(matches!(enum_not_found, Error::DatatypeNotFound(_, _, _)));

        // Add a new version of the system package into the store underlying the cache.
        inner.write().unwrap().replace(
            addr("0x1"),
            cached_package(2, BTreeMap::new(), &build_package("s1"), &s1_types()),
        );

        // Evict the package from the cache
        resolver.package_store().evict([addr("0x1")]);

        let struct_layout = resolver.type_layout(type_("0x1::m::T1")).await.unwrap();
        let enum_layout = resolver.type_layout(type_("0x1::m::E1")).await.unwrap();
        insta::assert_snapshot!(fmt(struct_layout, enum_layout));
    }

    #[tokio::test]
    async fn test_caching() {
        let (inner, cache) = package_cache([
            (1, build_package("a0"), a0_types()),
            (1, build_package("s0"), s0_types()),
        ]);
        let resolver = Resolver::new(cache);

        assert_eq!(inner.read().unwrap().fetches, 0);
        let l0 = resolver.type_layout(type_("0xa0::m::T0")).await.unwrap();

        // Load A0.
        assert_eq!(inner.read().unwrap().fetches, 1);

        // Layouts are the same, no need to reload the package.
        let l1 = resolver.type_layout(type_("0xa0::m::T0")).await.unwrap();
        assert_eq!(format!("{l0}"), format!("{l1}"));
        assert_eq!(inner.read().unwrap().fetches, 1);

        // Different type, but same package, so no extra fetch.
        let l2 = resolver.type_layout(type_("0xa0::m::T2")).await.unwrap();
        assert_ne!(format!("{l0}"), format!("{l2}"));
        assert_eq!(inner.read().unwrap().fetches, 1);

        // Enum types won't trigger a fetch either.
        resolver.type_layout(type_("0xa0::m::E0")).await.unwrap();
        assert_eq!(inner.read().unwrap().fetches, 1);

        // New package to load.
        let l3 = resolver.type_layout(type_("0x1::m::T0")).await.unwrap();
        assert_eq!(inner.read().unwrap().fetches, 2);

        // Reload the same system package type, it gets fetched from cache
        let l4 = resolver.type_layout(type_("0x1::m::T0")).await.unwrap();
        assert_eq!(format!("{l3}"), format!("{l4}"));
        assert_eq!(inner.read().unwrap().fetches, 2);

        // Reload a same system package type (enum), which will cause a version check.
        let el4 = resolver.type_layout(type_("0x1::m::E0")).await.unwrap();
        assert_ne!(format!("{el4}"), format!("{l4}"));
        assert_eq!(inner.read().unwrap().fetches, 2);

        // Upgrade the system package
        inner.write().unwrap().replace(
            addr("0x1"),
            cached_package(2, BTreeMap::new(), &build_package("s1"), &s1_types()),
        );

        // Evict the package from the cache
        resolver.package_store().evict([addr("0x1")]);

        // Reload the system system type again. It will be refetched (even though the type is the
        // same as before). This usage pattern (layouts for system types) is why a layout cache
        // would be particularly helpful (future optimisation).
        let l5 = resolver.type_layout(type_("0x1::m::T0")).await.unwrap();
        assert_eq!(format!("{l4}"), format!("{l5}"));
        assert_eq!(inner.read().unwrap().fetches, 3);
    }

    #[tokio::test]
    async fn test_layout_err_not_a_package() {
        let (_, cache) = package_cache([(1, build_package("a0"), a0_types())]);
        let resolver = Resolver::new(cache);
        let err = resolver
            .type_layout(type_("0x42::m::T0"))
            .await
            .unwrap_err();
        assert!(matches!(err, Error::PackageNotFound(_)));
    }

    #[tokio::test]
    async fn test_layout_err_no_module() {
        let (_, cache) = package_cache([(1, build_package("a0"), a0_types())]);
        let resolver = Resolver::new(cache);
        let err = resolver
            .type_layout(type_("0xa0::l::T0"))
            .await
            .unwrap_err();
        assert!(matches!(err, Error::ModuleNotFound(_, _)));
    }

    #[tokio::test]
    async fn test_layout_err_no_struct() {
        let (_, cache) = package_cache([(1, build_package("a0"), a0_types())]);
        let resolver = Resolver::new(cache);

        let err = resolver
            .type_layout(type_("0xa0::m::T9"))
            .await
            .unwrap_err();
        assert!(matches!(err, Error::DatatypeNotFound(_, _, _)));
    }

    #[tokio::test]
    async fn test_layout_err_type_arity() {
        let (_, cache) = package_cache([(1, build_package("a0"), a0_types())]);
        let resolver = Resolver::new(cache);

        // Too few
        let err = resolver
            .type_layout(type_("0xa0::m::T1<u8>"))
            .await
            .unwrap_err();
        assert!(matches!(err, Error::TypeArityMismatch(2, 1)));

        // Too many
        let err = resolver
            .type_layout(type_("0xa0::m::T1<u8, u16, u32>"))
            .await
            .unwrap_err();
        assert!(matches!(err, Error::TypeArityMismatch(2, 3)));
    }

    #[tokio::test]
    async fn test_structs() {
        let (_, cache) = package_cache([(1, build_package("a0"), a0_types())]);
        let a0 = cache.fetch(addr("0xa0")).await.unwrap();
        let m = a0.module("m").unwrap();

        assert_eq!(
            m.structs(None, None).collect::<Vec<_>>(),
            vec!["T0", "T1", "T2"],
        );

        assert_eq!(m.structs(None, Some("T1")).collect::<Vec<_>>(), vec!["T0"],);

        assert_eq!(
            m.structs(Some("T0"), Some("T2")).collect::<Vec<_>>(),
            vec!["T1"],
        );

        assert_eq!(m.structs(Some("T1"), None).collect::<Vec<_>>(), vec!["T2"],);

        let t0 = m.struct_def("T0").unwrap().unwrap();
        let t1 = m.struct_def("T1").unwrap().unwrap();
        let t2 = m.struct_def("T2").unwrap().unwrap();

        insta::assert_snapshot!(format!(
            "a0::m::T0: {t0:#?}\n\
             a0::m::T1: {t1:#?}\n\
             a0::m::T2: {t2:#?}",
        ));
    }

    #[tokio::test]
    async fn test_enums() {
        let (_, cache) = package_cache([(1, build_package("a0"), a0_types())]);
        let a0 = cache
            .fetch(AccountAddress::from_str("0xa0").unwrap())
            .await
            .unwrap();
        let m = a0.module("m").unwrap();

        assert_eq!(
            m.enums(None, None).collect::<Vec<_>>(),
            vec!["E0", "E1", "E2"],
        );

        assert_eq!(m.enums(None, Some("E1")).collect::<Vec<_>>(), vec!["E0"],);

        assert_eq!(
            m.enums(Some("E0"), Some("E2")).collect::<Vec<_>>(),
            vec!["E1"],
        );

        assert_eq!(m.enums(Some("E1"), None).collect::<Vec<_>>(), vec!["E2"],);

        let e0 = m.enum_def("E0").unwrap().unwrap();
        let e1 = m.enum_def("E1").unwrap().unwrap();
        let e2 = m.enum_def("E2").unwrap().unwrap();

        insta::assert_snapshot!(format!(
            "a0::m::E0: {e0:#?}\n\
             a0::m::E1: {e1:#?}\n\
             a0::m::E2: {e2:#?}",
        ));
    }

    #[tokio::test]
    async fn test_functions() {
        let (_, cache) = package_cache([
            (1, build_package("a0"), a0_types()),
            (2, build_package("a1"), a1_types()),
            (1, build_package("b0"), b0_types()),
            (1, build_package("c0"), c0_types()),
        ]);

        let c0 = cache.fetch(addr("0xc0")).await.unwrap();
        let m = c0.module("m").unwrap();

        assert_eq!(
            m.functions(None, None).collect::<Vec<_>>(),
            vec!["bar", "baz", "foo"],
        );

        assert_eq!(
            m.functions(None, Some("baz")).collect::<Vec<_>>(),
            vec!["bar"],
        );

        assert_eq!(
            m.functions(Some("bar"), Some("foo")).collect::<Vec<_>>(),
            vec!["baz"],
        );

        assert_eq!(
            m.functions(Some("baz"), None).collect::<Vec<_>>(),
            vec!["foo"],
        );

        let foo = m.function_def("foo").unwrap().unwrap();
        let bar = m.function_def("bar").unwrap().unwrap();
        let baz = m.function_def("baz").unwrap().unwrap();

        insta::assert_snapshot!(format!(
            "c0::m::foo: {foo:#?}\n\
             c0::m::bar: {bar:#?}\n\
             c0::m::baz: {baz:#?}"
        ));
    }

    #[tokio::test]
    async fn test_function_parameters() {
        let (_, cache) = package_cache([
            (1, build_package("a0"), a0_types()),
            (2, build_package("a1"), a1_types()),
            (1, build_package("b0"), b0_types()),
            (1, build_package("c0"), c0_types()),
        ]);

        let resolver = Resolver::new(cache);
        let c0 = addr("0xc0");

        let foo = resolver.function_signature(c0, "m", "foo").await.unwrap();
        let bar = resolver.function_signature(c0, "m", "bar").await.unwrap();
        let baz = resolver.function_signature(c0, "m", "baz").await.unwrap();

        insta::assert_snapshot!(format!(
            "c0::m::foo: {foo:#?}\n\
             c0::m::bar: {bar:#?}\n\
             c0::m::baz: {baz:#?}"
        ));
    }

    #[tokio::test]
    async fn test_signature_instantiation() {
        use OpenSignatureBody as O;
        use TypeInput as T;

        let sig = O::Datatype(
            key("0x2::table::Table"),
            vec![
                O::TypeParameter(1),
                O::Vector(Box::new(O::Datatype(
                    key("0x1::option::Option"),
                    vec![O::TypeParameter(0)],
                ))),
            ],
        );

        insta::assert_debug_snapshot!(sig.instantiate(&[T::U64, T::Bool]).unwrap());
    }

    #[tokio::test]
    async fn test_signature_instantiation_error() {
        use OpenSignatureBody as O;
        use TypeInput as T;

        let sig = O::Datatype(
            key("0x2::table::Table"),
            vec![
                O::TypeParameter(1),
                O::Vector(Box::new(O::Datatype(
                    key("0x1::option::Option"),
                    vec![O::TypeParameter(99)],
                ))),
            ],
        );

        insta::assert_snapshot!(
            sig.instantiate(&[T::U64, T::Bool]).unwrap_err(),
            @"Type Parameter 99 out of bounds (2)"
        );
    }

    /// Primitive types should have the expected primitive abilities
    #[tokio::test]
    async fn test_primitive_abilities() {
        use Ability as A;
        use AbilitySet as S;

        let (_, cache) = package_cache([]);
        let resolver = Resolver::new(cache);

        for prim in ["address", "bool", "u8", "u16", "u32", "u64", "u128", "u256"] {
            assert_eq!(
                resolver.abilities(type_(prim)).await.unwrap(),
                S::EMPTY | A::Copy | A::Drop | A::Store,
                "Unexpected primitive abilities for: {prim}",
            );
        }
    }

    /// Generic type abilities depend on the abilities of their type parameters.
    #[tokio::test]
    async fn test_simple_generic_abilities() {
        use Ability as A;
        use AbilitySet as S;

        let (_, cache) = package_cache([
            (1, build_package("sui"), sui_types()),
            (1, build_package("d0"), d0_types()),
        ]);
        let resolver = Resolver::new(cache);

        let a1 = resolver
            .abilities(type_("0xd0::m::T<u32, u64>"))
            .await
            .unwrap();
        assert_eq!(a1, S::EMPTY | A::Copy | A::Drop | A::Store);

        let a2 = resolver
            .abilities(type_("0xd0::m::T<0xd0::m::S, u64>"))
            .await
            .unwrap();
        assert_eq!(a2, S::EMPTY | A::Drop | A::Store);

        let a3 = resolver
            .abilities(type_("0xd0::m::T<0xd0::m::R, 0xd0::m::S>"))
            .await
            .unwrap();
        assert_eq!(a3, S::EMPTY | A::Drop);

        let a4 = resolver
            .abilities(type_("0xd0::m::T<0xd0::m::Q, 0xd0::m::R>"))
            .await
            .unwrap();
        assert_eq!(a4, S::EMPTY);
    }

    /// Generic abilities also need to handle nested type parameters
    #[tokio::test]
    async fn test_nested_generic_abilities() {
        use Ability as A;
        use AbilitySet as S;

        let (_, cache) = package_cache([
            (1, build_package("sui"), sui_types()),
            (1, build_package("d0"), d0_types()),
        ]);
        let resolver = Resolver::new(cache);

        let a1 = resolver
            .abilities(type_("0xd0::m::T<0xd0::m::T<0xd0::m::R, u32>, u64>"))
            .await
            .unwrap();
        assert_eq!(a1, S::EMPTY | A::Copy | A::Drop);
    }

    /// Key is different from other abilities in that it requires fields to have `store`, rather
    /// than itself.
    #[tokio::test]
    async fn test_key_abilities() {
        use Ability as A;
        use AbilitySet as S;

        let (_, cache) = package_cache([
            (1, build_package("sui"), sui_types()),
            (1, build_package("d0"), d0_types()),
        ]);
        let resolver = Resolver::new(cache);

        let a1 = resolver
            .abilities(type_("0xd0::m::O<u32, u64>"))
            .await
            .unwrap();
        assert_eq!(a1, S::EMPTY | A::Key | A::Store);

        let a2 = resolver
            .abilities(type_("0xd0::m::O<0xd0::m::S, u64>"))
            .await
            .unwrap();
        assert_eq!(a2, S::EMPTY | A::Key | A::Store);

        // We would not be able to get an instance of this type, but in case the question is asked,
        // its abilities would be empty.
        let a3 = resolver
            .abilities(type_("0xd0::m::O<0xd0::m::R, u64>"))
            .await
            .unwrap();
        assert_eq!(a3, S::EMPTY);

        // Key does not propagate up by itself, so this type is also uninhabitable.
        let a4 = resolver
            .abilities(type_("0xd0::m::O<0xd0::m::P, u32>"))
            .await
            .unwrap();
        assert_eq!(a4, S::EMPTY);
    }

    /// Phantom types don't impact abilities
    #[tokio::test]
    async fn test_phantom_abilities() {
        use Ability as A;
        use AbilitySet as S;

        let (_, cache) = package_cache([
            (1, build_package("sui"), sui_types()),
            (1, build_package("d0"), d0_types()),
        ]);
        let resolver = Resolver::new(cache);

        let a1 = resolver
            .abilities(type_("0xd0::m::O<u32, 0xd0::m::R>"))
            .await
            .unwrap();
        assert_eq!(a1, S::EMPTY | A::Key | A::Store);
    }

    #[tokio::test]
    async fn test_err_ability_arity() {
        let (_, cache) = package_cache([
            (1, build_package("sui"), sui_types()),
            (1, build_package("d0"), d0_types()),
        ]);
        let resolver = Resolver::new(cache);

        // Too few
        let err = resolver
            .abilities(type_("0xd0::m::T<u8>"))
            .await
            .unwrap_err();
        assert!(matches!(err, Error::TypeArityMismatch(2, 1)));

        // Too many
        let err = resolver
            .abilities(type_("0xd0::m::T<u8, u16, u32>"))
            .await
            .unwrap_err();
        assert!(matches!(err, Error::TypeArityMismatch(2, 3)));
    }

    #[tokio::test]
    async fn test_err_ability_signer() {
        let (_, cache) = package_cache([]);
        let resolver = Resolver::new(cache);

        let err = resolver.abilities(type_("signer")).await.unwrap_err();
        assert!(matches!(err, Error::UnexpectedSigner));
    }

    #[tokio::test]
    async fn test_err_too_many_type_params() {
        let (_, cache) = package_cache([
            (1, build_package("sui"), sui_types()),
            (1, build_package("d0"), d0_types()),
        ]);

        let resolver = Resolver::new_with_limits(
            cache,
            Limits {
                max_type_argument_width: 1,
                max_type_argument_depth: 100,
                max_type_nodes: 100,
                max_move_value_depth: 100,
            },
        );

        let err = resolver
            .abilities(type_("0xd0::m::O<u32, u64>"))
            .await
            .unwrap_err();
        assert!(matches!(err, Error::TooManyTypeParams(1, 2)));
    }

    #[tokio::test]
    async fn test_err_too_many_type_nodes() {
        use Ability as A;
        use AbilitySet as S;

        let (_, cache) = package_cache([
            (1, build_package("sui"), sui_types()),
            (1, build_package("d0"), d0_types()),
        ]);

        let resolver = Resolver::new_with_limits(
            cache,
            Limits {
                max_type_argument_width: 100,
                max_type_argument_depth: 100,
                max_type_nodes: 2,
                max_move_value_depth: 100,
            },
        );

        // This request is OK, because one of O's type parameters is phantom, so we can avoid
        // loading its definition.
        let a1 = resolver
            .abilities(type_("0xd0::m::O<0xd0::m::S, 0xd0::m::Q>"))
            .await
            .unwrap();
        assert_eq!(a1, S::EMPTY | A::Key | A::Store);

        // But this request will hit the limit
        let err = resolver
            .abilities(type_("0xd0::m::T<0xd0::m::P, 0xd0::m::Q>"))
            .await
            .unwrap_err();
        assert!(matches!(err, Error::TooManyTypeNodes(2, _)));
    }

    #[tokio::test]
    async fn test_err_type_param_nesting() {
        use Ability as A;
        use AbilitySet as S;

        let (_, cache) = package_cache([
            (1, build_package("sui"), sui_types()),
            (1, build_package("d0"), d0_types()),
        ]);

        let resolver = Resolver::new_with_limits(
            cache,
            Limits {
                max_type_argument_width: 100,
                max_type_argument_depth: 2,
                max_type_nodes: 100,
                max_move_value_depth: 100,
            },
        );

        // This request is OK, because one of O's type parameters is phantom, so we can avoid
        // loading its definition.
        let a1 = resolver
            .abilities(type_(
                "0xd0::m::O<0xd0::m::S, 0xd0::m::T<vector<u32>, vector<u64>>>",
            ))
            .await
            .unwrap();
        assert_eq!(a1, S::EMPTY | A::Key | A::Store);

        // But this request will hit the limit
        let err = resolver
            .abilities(type_("vector<0xd0::m::T<0xd0::m::O<u64, u32>, u16>>"))
            .await
            .unwrap_err();
        assert!(matches!(err, Error::TypeParamNesting(2, _)));
    }

    #[tokio::test]
    async fn test_pure_input_layouts() {
        use CallArg as I;
        use ObjectArg::ImmOrOwnedObject as O;
        use TypeTag as T;

        let (_, cache) = package_cache([
            (1, build_package("std"), std_types()),
            (1, build_package("sui"), sui_types()),
            (1, build_package("e0"), e0_types()),
        ]);

        let resolver = Resolver::new(cache);

        // Helper function to generate a PTB calling 0xe0::m::foo.
        fn ptb(t: TypeTag, y: CallArg) -> ProgrammableTransaction {
            ProgrammableTransaction {
                inputs: vec![
                    I::Object(O(random_object_ref())),
                    I::Pure(bcs::to_bytes(&42u64).unwrap()),
                    I::Object(O(random_object_ref())),
                    y,
                    I::Object(O(random_object_ref())),
                    I::Pure(bcs::to_bytes("hello").unwrap()),
                    I::Pure(bcs::to_bytes("world").unwrap()),
                ],
                commands: vec![Command::move_call(
                    addr("0xe0").into(),
                    ident_str!("m").to_owned(),
                    ident_str!("foo").to_owned(),
                    vec![t],
                    (0..=6).map(Argument::Input).collect(),
                )],
            }
        }

        let ptb_u64 = ptb(T::U64, I::Pure(bcs::to_bytes(&1u64).unwrap()));

        let ptb_opt = ptb(
            TypeTag::Struct(Box::new(StructTag {
                address: addr("0x1"),
                module: ident_str!("option").to_owned(),
                name: ident_str!("Option").to_owned(),
                type_params: vec![TypeTag::U64],
            })),
            I::Pure(bcs::to_bytes(&[vec![1u64], vec![], vec![3]]).unwrap()),
        );

        let ptb_obj = ptb(
            TypeTag::Struct(Box::new(StructTag {
                address: addr("0xe0"),
                module: ident_str!("m").to_owned(),
                name: ident_str!("O").to_owned(),
                type_params: vec![],
            })),
            I::Object(O(random_object_ref())),
        );

        let inputs_u64 = resolver.pure_input_layouts(&ptb_u64).await.unwrap();
        let inputs_opt = resolver.pure_input_layouts(&ptb_opt).await.unwrap();
        let inputs_obj = resolver.pure_input_layouts(&ptb_obj).await.unwrap();

        // Make the output format a little nicer for the snapshot
        let mut output = "---\n".to_string();
        for inputs in [inputs_u64, inputs_opt, inputs_obj] {
            for input in inputs {
                if let Some(layout) = input {
                    output += &format!("{layout:#}\n");
                } else {
                    output += "???\n";
                }
            }
            output += "---\n";
        }

        insta::assert_snapshot!(output);
    }

    /// Like the test above, but the inputs are re-used, which we want to detect (but is fine
    /// because they are assigned the same type at each usage).
    #[tokio::test]
    async fn test_pure_input_layouts_overlapping() {
        use CallArg as I;
        use ObjectArg::ImmOrOwnedObject as O;
        use TypeTag as T;

        let (_, cache) = package_cache([
            (1, build_package("std"), std_types()),
            (1, build_package("sui"), sui_types()),
            (1, build_package("e0"), e0_types()),
        ]);

        let resolver = Resolver::new(cache);

        // Helper function to generate a PTB calling 0xe0::m::foo.
        let ptb = ProgrammableTransaction {
            inputs: vec![
                I::Object(O(random_object_ref())),
                I::Pure(bcs::to_bytes(&42u64).unwrap()),
                I::Object(O(random_object_ref())),
                I::Pure(bcs::to_bytes(&43u64).unwrap()),
                I::Object(O(random_object_ref())),
                I::Pure(bcs::to_bytes("hello").unwrap()),
                I::Pure(bcs::to_bytes("world").unwrap()),
            ],
            commands: vec![
                Command::move_call(
                    addr("0xe0").into(),
                    ident_str!("m").to_owned(),
                    ident_str!("foo").to_owned(),
                    vec![T::U64],
                    (0..=6).map(Argument::Input).collect(),
                ),
                Command::move_call(
                    addr("0xe0").into(),
                    ident_str!("m").to_owned(),
                    ident_str!("foo").to_owned(),
                    vec![T::U64],
                    (0..=6).map(Argument::Input).collect(),
                ),
            ],
        };

        let inputs = resolver.pure_input_layouts(&ptb).await.unwrap();

        // Make the output format a little nicer for the snapshot
        let mut output = String::new();
        for input in inputs {
            if let Some(layout) = input {
                output += &format!("{layout:#}\n");
            } else {
                output += "???\n";
            }
        }

        insta::assert_snapshot!(output);
    }
    #[tokio::test]
    async fn test_pure_input_layouts_conflicting() {
        use CallArg as I;
        use ObjectArg::ImmOrOwnedObject as O;
        use TypeInput as TI;
        use TypeTag as T;

        let (_, cache) = package_cache([
            (1, build_package("std"), std_types()),
            (1, build_package("sui"), sui_types()),
            (1, build_package("e0"), e0_types()),
        ]);

        let resolver = Resolver::new(cache);

        let ptb = ProgrammableTransaction {
            inputs: vec![
                I::Object(O(random_object_ref())),
                I::Pure(bcs::to_bytes(&42u64).unwrap()),
                I::Object(O(random_object_ref())),
                I::Pure(bcs::to_bytes(&43u64).unwrap()),
                I::Object(O(random_object_ref())),
                I::Pure(bcs::to_bytes("hello").unwrap()),
                I::Pure(bcs::to_bytes("world").unwrap()),
            ],
            commands: vec![
                Command::move_call(
                    addr("0xe0").into(),
                    ident_str!("m").to_owned(),
                    ident_str!("foo").to_owned(),
                    vec![T::U64],
                    (0..=6).map(Argument::Input).collect(),
                ),
                // This command is using the input that was previously used as a U64, but now as a
                // U32, which will cause an error.
                Command::MakeMoveVec(Some(TI::U32), vec![Argument::Input(3)]),
            ],
        };

        insta::assert_snapshot!(
            resolver.pure_input_layouts(&ptb).await.unwrap_err(),
            @"Conflicting types for input 3: u64 and u32"
        );
    }

    /***** Test Helpers ***************************************************************************/

    type TypeOriginTable = Vec<DatatypeKey>;

    fn a0_types() -> TypeOriginTable {
        vec![
            datakey("0xa0", "m", "T0"),
            datakey("0xa0", "m", "T1"),
            datakey("0xa0", "m", "T2"),
            datakey("0xa0", "m", "E0"),
            datakey("0xa0", "m", "E1"),
            datakey("0xa0", "m", "E2"),
            datakey("0xa0", "n", "T0"),
            datakey("0xa0", "n", "E0"),
        ]
    }

    fn a1_types() -> TypeOriginTable {
        let mut types = a0_types();

        types.extend([
            datakey("0xa1", "m", "T3"),
            datakey("0xa1", "m", "T4"),
            datakey("0xa1", "n", "T1"),
            datakey("0xa1", "m", "E3"),
            datakey("0xa1", "m", "E4"),
            datakey("0xa1", "n", "E1"),
        ]);

        types
    }

    fn b0_types() -> TypeOriginTable {
        vec![datakey("0xb0", "m", "T0"), datakey("0xb0", "m", "E0")]
    }

    fn c0_types() -> TypeOriginTable {
        vec![datakey("0xc0", "m", "T0"), datakey("0xc0", "m", "E0")]
    }

    fn d0_types() -> TypeOriginTable {
        vec![
            datakey("0xd0", "m", "O"),
            datakey("0xd0", "m", "P"),
            datakey("0xd0", "m", "Q"),
            datakey("0xd0", "m", "R"),
            datakey("0xd0", "m", "S"),
            datakey("0xd0", "m", "T"),
            datakey("0xd0", "m", "EO"),
            datakey("0xd0", "m", "EP"),
            datakey("0xd0", "m", "EQ"),
            datakey("0xd0", "m", "ER"),
            datakey("0xd0", "m", "ES"),
            datakey("0xd0", "m", "ET"),
        ]
    }

    fn e0_types() -> TypeOriginTable {
        vec![datakey("0xe0", "m", "O")]
    }

    fn s0_types() -> TypeOriginTable {
        vec![datakey("0x1", "m", "T0"), datakey("0x1", "m", "E0")]
    }

    fn s1_types() -> TypeOriginTable {
        let mut types = s0_types();

        types.extend([datakey("0x1", "m", "T1"), datakey("0x1", "m", "E1")]);

        types
    }

    fn sui_types() -> TypeOriginTable {
        vec![datakey("0x2", "object", "UID")]
    }

    fn std_types() -> TypeOriginTable {
        vec![
            datakey("0x1", "ascii", "String"),
            datakey("0x1", "option", "Option"),
            datakey("0x1", "string", "String"),
        ]
    }

    /// Build an in-memory package cache from locally compiled packages.  Assumes that all packages
    /// in `packages` are published (all modules have a non-zero package address and all packages
    /// have a 'published-at' address), and their transitive dependencies are also in `packages`.
    fn package_cache(
        packages: impl IntoIterator<Item = (u64, CompiledPackage, TypeOriginTable)>,
    ) -> (
        Arc<RwLock<InnerStore>>,
        PackageStoreWithLruCache<InMemoryPackageStore>,
    ) {
        let packages_by_storage_id: BTreeMap<AccountAddress, _> = packages
            .into_iter()
            .map(|(version, package, origins)| {
                (package_storage_id(&package), (version, package, origins))
            })
            .collect();

        let packages = packages_by_storage_id
            .iter()
            .map(|(&storage_id, (version, compiled_package, origins))| {
                let linkage = compiled_package
                    .dependency_ids
                    .published
                    .values()
                    .map(|dep_id| {
                        let storage_id = AccountAddress::from(*dep_id);
                        let runtime_id = package_runtime_id(
                            &packages_by_storage_id
                                .get(&storage_id)
                                .unwrap_or_else(|| panic!("Dependency {storage_id} not in store"))
                                .1,
                        );

                        (runtime_id, storage_id)
                    })
                    .collect();

                let package = cached_package(*version, linkage, compiled_package, origins);
                (storage_id, package)
            })
            .collect();

        let inner = Arc::new(RwLock::new(InnerStore {
            packages,
            fetches: 0,
        }));

        let store = InMemoryPackageStore {
            inner: inner.clone(),
        };

        (inner, PackageStoreWithLruCache::new(store))
    }

    fn cached_package(
        version: u64,
        linkage: Linkage,
        package: &CompiledPackage,
        origins: &TypeOriginTable,
    ) -> Package {
        let storage_id = package_storage_id(package);
        let runtime_id = package_runtime_id(package);
        let version = SequenceNumber::from_u64(version);

        let mut modules = BTreeMap::new();
        for unit in &package.package.root_compiled_units {
            let NamedCompiledModule { name, module, .. } = &unit.unit;

            let origins = origins
                .iter()
                .filter(|key| key.module == name.as_str())
                .map(|key| (key.name.to_string(), key.package))
                .collect();

            let module = match Module::read(module.clone(), origins) {
                Ok(module) => module,
                Err(struct_) => {
                    panic!("Missing type origin for {}::{struct_}", module.self_id());
                }
            };

            modules.insert(name.to_string(), module);
        }

        Package {
            storage_id,
            runtime_id,
            linkage,
            version,
            modules,
        }
    }

    fn package_storage_id(package: &CompiledPackage) -> AccountAddress {
        AccountAddress::from(*package.published_at.as_ref().unwrap_or_else(|_| {
            panic!(
                "Package {} doesn't have published-at set",
                package.package.compiled_package_info.package_name,
            )
        }))
    }

    fn package_runtime_id(package: &CompiledPackage) -> AccountAddress {
        *package
            .published_root_module()
            .expect("No compiled module")
            .address()
    }

    fn build_package(dir: &str) -> CompiledPackage {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.extend(["tests", "packages", dir]);
        BuildConfig::new_for_testing().build(&path).unwrap()
    }

    fn addr(a: &str) -> AccountAddress {
        AccountAddress::from_str(a).unwrap()
    }

    fn datakey(a: &str, m: &'static str, n: &'static str) -> DatatypeKey {
        DatatypeKey {
            package: addr(a),
            module: m.into(),
            name: n.into(),
        }
    }

    fn type_(t: &str) -> TypeTag {
        TypeTag::from_str(t).unwrap()
    }

    fn key(t: &str) -> DatatypeKey {
        let tag = StructTag::from_str(t).unwrap();
        DatatypeRef::from(&tag).as_key()
    }

    struct InMemoryPackageStore {
        /// All the contents are stored in an `InnerStore` that can be probed and queried from
        /// outside.
        inner: Arc<RwLock<InnerStore>>,
    }

    struct InnerStore {
        packages: BTreeMap<AccountAddress, Package>,
        fetches: usize,
    }

    #[async_trait]
    impl PackageStore for InMemoryPackageStore {
        async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>> {
            let mut inner = self.inner.as_ref().write().unwrap();
            inner.fetches += 1;
            inner
                .packages
                .get(&id)
                .cloned()
                .ok_or_else(|| Error::PackageNotFound(id))
                .map(Arc::new)
        }
    }

    impl InnerStore {
        fn replace(&mut self, id: AccountAddress, package: Package) {
            self.packages.insert(id, package);
        }
    }
}
