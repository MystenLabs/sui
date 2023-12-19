// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use lru::LruCache;
use move_binary_format::file_format::{
    AbilitySet, FunctionDefinitionIndex, Signature, SignatureIndex, StructTypeParameter, Visibility,
};
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use std::{borrow::Cow, collections::BTreeMap};

use crate::error::Error;
use move_binary_format::errors::Location;
use move_binary_format::{
    access::ModuleAccess,
    file_format::{
        SignatureToken, StructDefinitionIndex, StructFieldInformation, StructHandleIndex,
        TableIndex,
    },
    CompiledModule,
};
use move_core_types::{
    account_address::AccountAddress,
    annotated_value::{MoveFieldLayout, MoveStructLayout, MoveTypeLayout},
    language_storage::{StructTag, TypeTag},
};
use sui_types::move_package::TypeOrigin;
use sui_types::object::Object;
use sui_types::{base_types::SequenceNumber, is_system_package, Identifier};

pub mod error;

// TODO Move to ServiceConfig

const PACKAGE_CACHE_SIZE: NonZeroUsize = unsafe { NonZeroUsize::new_unchecked(1024) };

pub type Result<T> = std::result::Result<T, Error>;

/// The Resolver is responsible for providing information about types. It relies on its internal
/// `package_store` to load packages and then type definitions from those packages.
#[derive(Debug)]
pub struct Resolver<S> {
    package_store: S,
    // TODO Remove when limits all implemented
    #[allow(dead_code)]
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

    /// The version this package was loaded at -- necessary for cache invalidation of system
    /// packages.
    version: SequenceNumber,

    modules: BTreeMap<String, Module>,
}

type Linkage = BTreeMap<AccountAddress, AccountAddress>;

#[derive(Clone, Debug)]
pub struct Module {
    bytecode: CompiledModule,

    /// Index mapping struct names to their defining ID, and the index for their definition in the
    /// bytecode, to speed up definition lookups.
    struct_index: BTreeMap<String, (AccountAddress, StructDefinitionIndex)>,

    /// Index mapping function names to the index for their definition in the bytecode, to speed up
    /// definition lookups.
    function_index: BTreeMap<String, FunctionDefinitionIndex>,
}

/// Deserialized representation of a struct definition.
#[derive(Debug)]
pub struct StructDef {
    /// The storage ID of the package that first introduced this type.
    pub defining_id: AccountAddress,

    /// This type's abilities.
    pub abilities: AbilitySet,

    /// Ability constraints and phantom status for type parameters
    pub type_params: Vec<StructTypeParameter>,

    /// Serialized representation of fields (names and deserialized signatures). Signatures refer to
    /// packages at their runtime IDs (not their storage ID or defining ID).
    pub fields: Vec<(String, OpenSignatureBody)>,
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
pub struct StructRef<'m, 'n> {
    pub package: AccountAddress,
    pub module: Cow<'m, str>,
    pub name: Cow<'n, str>,
}

/// A `StructRef` that owns its strings.
pub type StructKey = StructRef<'static, 'static>;

#[derive(Clone, Debug)]
pub enum Reference {
    Immutable,
    Mutable,
}

#[derive(Clone, Debug)]
pub struct OpenSignature {
    pub ref_: Option<Reference>,
    pub body: OpenSignatureBody,
}

/// Deserialized representation of a type signature that could appear as a field type for a struct.
/// Signatures refer to structs at their runtime IDs and can contain references to free type
/// parameters but will not contain reference types.
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
    Struct(StructKey, Vec<OpenSignatureBody>),
    TypeParameter(u16),
}

/// Information necessary to convert a type tag into a type layout.
#[derive(Debug, Default)]
struct ResolutionContext<'l> {
    /// Definitions (field information) for structs referred to by types added to this context.
    structs: BTreeMap<StructKey, StructDef>,
    /// Limits configuration from the calling resolver.
    limits: Option<&'l Limits>,
}

/// Interface to abstract over access to a store of live packages.  Used to override the default
/// store during testing.
#[async_trait]
pub trait PackageStore: Send + Sync + 'static {
    /// Latest version of the object at `id`.
    async fn version(&self, id: AccountAddress) -> Result<SequenceNumber>;
    /// Read package contents. Fails if `id` is not an object, not a package, or is malformed in
    /// some way.
    async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>>;
}

macro_rules! as_ref_impl {
    ($type:ty) => {
        #[async_trait]
        impl PackageStore for $type {
            async fn version(&self, id: AccountAddress) -> Result<SequenceNumber> {
                self.as_ref().version(id).await
            }
            async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>> {
                self.as_ref().fetch(id).await
            }
        }
    };
}

as_ref_impl!(Arc<dyn PackageStore>);
as_ref_impl!(Box<dyn PackageStore>);

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
        context.resolve_layout(&tag)
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
}

impl<T> PackageStoreWithLruCache<T> {
    pub fn new(inner: T) -> Self {
        let packages = Mutex::new(LruCache::new(PACKAGE_CACHE_SIZE));
        Self { packages, inner }
    }
}

#[async_trait]
impl<T: PackageStore> PackageStore for PackageStoreWithLruCache<T> {
    async fn version(&self, id: AccountAddress) -> Result<SequenceNumber> {
        self.inner.version(id).await
    }
    async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>> {
        let candidate = {
            // Release the lock after getting the package
            let mut packages = self.packages.lock().unwrap();
            packages.get(&id).map(Arc::clone)
        };

        // System packages can be invalidated in the cache if a newer version exists.
        match candidate {
            Some(package) if !is_system_package(id) => return Ok(package),
            Some(package) if self.version(id).await? <= package.version => return Ok(package),
            Some(_) | None => { /* nop */ }
        }

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
    pub fn read(object: &Object) -> Result<Self> {
        let storage_id = AccountAddress::from(object.id());
        let Some(package) = object.data.try_as_package() else {
            return Err(Error::NotAPackage(storage_id));
        };

        let mut type_origins: BTreeMap<String, BTreeMap<String, AccountAddress>> = BTreeMap::new();
        for TypeOrigin {
            module_name,
            struct_name,
            package,
        } in package.type_origin_table()
        {
            type_origins
                .entry(module_name.to_string())
                .or_default()
                .insert(struct_name.to_string(), AccountAddress::from(*package));
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

    fn struct_def(&self, module_name: &str, struct_name: &str) -> Result<StructDef> {
        let module = self.module(module_name)?;
        let Some(struct_def) = module.struct_def(struct_name)? else {
            return Err(Error::StructNotFound(
                self.storage_id,
                module_name.to_string(),
                struct_name.to_string(),
            ));
        };

        Ok(struct_def)
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
            let sh = bytecode.struct_handle_at(def.struct_handle);
            let struct_ = bytecode.identifier_at(sh.name).to_string();
            let index = StructDefinitionIndex::new(index as TableIndex);

            let Some(defining_id) = origins.remove(&struct_) else {
                return Err(struct_);
            };

            struct_index.insert(struct_, (defining_id, index));
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
    ) -> impl Iterator<Item = &str> + Clone {
        use std::ops::Bound as B;
        self.struct_index
            .range::<str, _>((
                after.map_or(B::Unbounded, B::Excluded),
                before.map_or(B::Unbounded, B::Excluded),
            ))
            .map(|(name, _)| name.as_str())
    }

    /// Get the struct definition corresponding to the struct with name `name` in this module.
    /// Returns `Ok(None)` if the struct cannot be found in this module, `Err(...)` if there was an
    /// error deserializing it, and `Ok(Some(def))` on success.
    pub fn struct_def(&self, name: &str) -> Result<Option<StructDef>> {
        let Some(&(defining_id, index)) = self.struct_index.get(name) else {
            return Ok(None);
        };

        let struct_def = self.bytecode.struct_def_at(index);
        let struct_handle = self.bytecode.struct_handle_at(struct_def.struct_handle);
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

        Ok(Some(StructDef {
            defining_id,
            abilities,
            type_params,
            fields,
        }))
    }

    /// Iterate over the functions with names strictly after `after` (or from the beginning), and
    /// strictly before `before` (or to the end).
    pub fn functions(
        &self,
        after: Option<&str>,
        before: Option<&str>,
    ) -> impl Iterator<Item = &str> + Clone {
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

            S::Struct(ix) => O::Struct(StructKey::read(*ix, bytecode), vec![]),
            S::StructInstantiation(ix, params) => O::Struct(
                StructKey::read(*ix, bytecode),
                params
                    .iter()
                    .map(|sig| OpenSignatureBody::read(sig, bytecode))
                    .collect::<Result<_>>()?,
            ),
        })
    }
}

impl<'m, 'n> StructRef<'m, 'n> {
    pub fn as_key(&self) -> StructKey {
        StructKey {
            package: self.package,
            module: self.module.to_string().into(),
            name: self.name.to_string().into(),
        }
    }
}

impl StructKey {
    fn read(ix: StructHandleIndex, bytecode: &CompiledModule) -> Self {
        let sh = bytecode.struct_handle_at(ix);
        let mh = bytecode.module_handle_at(sh.module);

        let package = *bytecode.address_identifier_at(mh.address);
        let module = bytecode.identifier_at(mh.name).to_string().into();
        let name = bytecode.identifier_at(sh.name).to_string().into();

        StructKey {
            package,
            module,
            name,
        }
    }
}

impl<'l> ResolutionContext<'l> {
    fn new(limits: Option<&'l Limits>) -> Self {
        ResolutionContext {
            structs: BTreeMap::new(),
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

        let mut frontier = vec![tag];
        while let Some(tag) = frontier.pop() {
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

                T::Vector(tag) => frontier.push(tag),

                T::Struct(s) => {
                    let context = store.fetch(s.address).await?;
                    let struct_def = context
                        .clone()
                        .struct_def(s.module.as_str(), s.name.as_str())?;

                    // Normalize `address` (the ID of a package that contains the definition of this
                    // struct) to be a runtime ID, because that's what the resolution context uses
                    // for keys.  Take care to do this before generating the key that is used to
                    // query and/or write into `self.structs.
                    s.address = context.runtime_id;
                    let key = StructRef::from(s.as_ref()).as_key();

                    if let Some(l) = self.limits {
                        let params = s.type_params.len();
                        if params > l.max_type_argument_width {
                            return Err(Error::TooManyTypeParams(
                                l.max_type_argument_width,
                                params,
                            ));
                        }
                    }

                    for (param, def) in s.type_params.iter_mut().zip(struct_def.type_params.iter())
                    {
                        if !def.is_phantom || visit_phantoms {
                            frontier.push(param)
                        }
                    }

                    if self.structs.contains_key(&key) {
                        continue;
                    }

                    if visit_fields {
                        for (_, sig) in &struct_def.fields {
                            self.add_signature(sig.clone(), store, &context).await?;
                        }
                    }

                    self.structs.insert(key, struct_def);
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

                O::Struct(key, params) => {
                    if let Some(l) = self.limits {
                        let params = params.len();
                        if params > l.max_type_argument_width {
                            return Err(Error::TooManyTypeParams(
                                l.max_type_argument_width,
                                params,
                            ));
                        }
                    }

                    frontier.extend(params.into_iter());

                    if self.structs.contains_key(&key) {
                        continue;
                    }

                    let storage_id = context.relocate(key.package)?;
                    let package = store.fetch(storage_id).await?;
                    let struct_def = package.struct_def(&key.module, &key.name)?;

                    frontier.extend(struct_def.fields.iter().map(|f| &f.1).cloned());
                    self.structs.insert(key.clone(), struct_def);
                }
            }
        }

        Ok(())
    }

    /// Translate a type `tag` into its layout using only the information contained in this context.
    /// Requires that the necessary information was added to the context through calls to
    /// `add_type_tag` and `add_signature` before being called.
    fn resolve_layout(&self, tag: &TypeTag) -> Result<MoveTypeLayout> {
        use MoveTypeLayout as L;
        use TypeTag as T;

        Ok(match tag {
            T::Signer => return Err(Error::UnexpectedSigner),

            T::Address => L::Address,
            T::Bool => L::Bool,
            T::U8 => L::U8,
            T::U16 => L::U16,
            T::U32 => L::U32,
            T::U64 => L::U64,
            T::U128 => L::U128,
            T::U256 => L::U256,

            T::Vector(tag) => L::Vector(Box::new(self.resolve_layout(tag)?)),

            T::Struct(s) => {
                // TODO (optimization): Could introduce a layout cache to further speed up
                // resolution.  Relevant entries in that cache would need to be gathered in the
                // ResolutionContext as it is built, and then used here to avoid the recursive
                // exploration.  This optimisation is complicated by the fact that in the cache,
                // these layouts are naturally keyed based on defining ID, but during resolution,
                // they are keyed by runtime IDs.

                // SAFETY: `add_type_tag` ensures `structs` has an element with this key.
                let key = StructRef::from(s.as_ref());
                let def = &self.structs[&key];

                let StructTag {
                    module,
                    name,
                    type_params,
                    ..
                } = s.as_ref();

                if def.type_params.len() != type_params.len() {
                    return Err(Error::TypeArityMismatch(
                        def.type_params.len(),
                        type_params.len(),
                    ));
                }

                // TODO (optimization): This could be made more efficient by only generating layouts
                // for non-phantom types.  This efficiency could be extended to the exploration
                // phase (i.e. only explore layouts of non-phantom types). But this optimisation is
                // complicated by the fact that we still need to create a correct type tag for a
                // phantom parameter, which is currently done by converting a type layout into a
                // tag.
                let param_layouts = type_params
                    .iter()
                    .map(|tag| self.resolve_layout(tag))
                    .collect::<Result<Vec<_>>>()?;

                // SAFETY: `param_layouts` contains `MoveTypeLayout`-s that are generated by this
                // `ResolutionContext`, which guarantees that struct layouts come with types, which
                // is necessary to avoid errors when converting layouts into type tags.
                let type_params = param_layouts
                    .iter()
                    .map(|layout| layout.try_into().unwrap())
                    .collect();

                let type_ = StructTag {
                    address: def.defining_id,
                    module: module.clone(),
                    name: name.clone(),
                    type_params,
                };

                let fields = def
                    .fields
                    .iter()
                    .map(|(name, sig)| {
                        Ok(MoveFieldLayout {
                            name: ident(name.as_str())?,
                            layout: self.resolve_signature(sig, &param_layouts)?,
                        })
                    })
                    .collect::<Result<_>>()?;

                L::Struct(MoveStructLayout { type_, fields })
            }
        })
    }

    /// Like `resolve_type_tag` but for signatures.  Needs to be provided the layouts of type
    /// parameters which are substituted when a type parameter is encountered.
    fn resolve_signature(
        &self,
        sig: &OpenSignatureBody,
        param_layouts: &Vec<MoveTypeLayout>,
    ) -> Result<MoveTypeLayout> {
        use MoveTypeLayout as L;
        use OpenSignatureBody as O;

        Ok(match sig {
            O::Address => L::Address,
            O::Bool => L::Bool,
            O::U8 => L::U8,
            O::U16 => L::U16,
            O::U32 => L::U32,
            O::U64 => L::U64,
            O::U128 => L::U128,
            O::U256 => L::U256,

            O::TypeParameter(ix) => param_layouts
                .get(*ix as usize)
                .ok_or_else(|| Error::TypeParamOOB(*ix, param_layouts.len()))
                .cloned()?,

            O::Vector(sig) => L::Vector(Box::new(
                self.resolve_signature(sig.as_ref(), param_layouts)?,
            )),

            O::Struct(key, params) => {
                // SAFETY: `add_signature` ensures `structs` has an element with this key.
                let def = &self.structs[key];

                let param_layouts = params
                    .iter()
                    .map(|sig| self.resolve_signature(sig, param_layouts))
                    .collect::<Result<Vec<_>>>()?;

                // SAFETY: `param_layouts` contains `MoveTypeLayout`-s that are generated by this
                // `ResolutionContext`, which guarantees that struct layouts come with types, which
                // is necessary to avoid errors when converting layouts into type tags.
                let type_params = param_layouts
                    .iter()
                    .map(|layout| layout.try_into().unwrap())
                    .collect();

                let type_ = StructTag {
                    address: def.defining_id,
                    module: ident(&key.module)?,
                    name: ident(&key.name)?,
                    type_params,
                };

                let fields = def
                    .fields
                    .iter()
                    .map(|(name, sig)| {
                        Ok(MoveFieldLayout {
                            name: ident(name.as_str())?,
                            layout: self.resolve_signature(sig, &param_layouts)?,
                        })
                    })
                    .collect::<Result<_>>()?;

                L::Struct(MoveStructLayout { type_, fields })
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
                // SAFETY: `add_type_tag` ensures `structs` has an element with this key.
                let key = StructRef::from(s.as_ref());
                let def = &self.structs[&key];

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
                .map_err(|e| Error::UnexpectedError(Box::new(e)))?
            }
        })
    }
}

impl<'s> From<&'s StructTag> for StructRef<'s, 's> {
    fn from(tag: &'s StructTag) -> Self {
        StructRef {
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

/// Read and deserialize a signature index (from function parameter or return types) into a vector
/// of signatures.
fn read_signature(idx: SignatureIndex, bytecode: &CompiledModule) -> Result<Vec<OpenSignature>> {
    let Signature(tokens) = bytecode.signature_at(idx);
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
    use std::sync::Arc;
    use std::{path::PathBuf, str::FromStr, sync::RwLock};

    use expect_test::expect;
    use move_compiler::compiled_unit::NamedCompiledModule;
    use sui_move_build::{BuildConfig, CompiledPackage};

    use super::*;

    /// Layout for a type that only refers to base types or other types in the same module.
    #[tokio::test]
    async fn test_simple_type() {
        let (_, cache) = package_cache([(1, build_package("a0"), a0_types())]);
        let package_resolver = Resolver::new(cache);
        let layout = package_resolver
            .type_layout(type_("0xa0::m::T0"))
            .await
            .unwrap();
        let expect = expect![[r#"
            struct 0xa0::m::T0 {
                b: bool,
                v: vector<struct 0xa0::m::T1<0xa0::m::T2, u128> {
                    a: address,
                    p: struct 0xa0::m::T2 {
                        x: u8,
                    },
                    q: vector<u128>,
                }>,
            }"#]];

        expect.assert_eq(&format!("{layout:#}"));
    }

    /// A type that refers to types from other modules in the same package.
    #[tokio::test]
    async fn test_cross_module() {
        let (_, cache) = package_cache([(1, build_package("a0"), a0_types())]);
        let resolver = Resolver::new(cache);
        let layout = resolver.type_layout(type_("0xa0::n::T0")).await.unwrap();
        let expect = expect![[r#"
            struct 0xa0::n::T0 {
                t: struct 0xa0::m::T1<u16, u32> {
                    a: address,
                    p: u16,
                    q: vector<u32>,
                },
                u: struct 0xa0::m::T2 {
                    x: u8,
                },
            }"#]];

        expect.assert_eq(&format!("{layout:#}"));
    }

    /// A type that refers to types from other modules in the same package.
    #[tokio::test]
    async fn test_cross_package() {
        let (_, cache) = package_cache([
            (1, build_package("a0"), a0_types()),
            (1, build_package("b0"), b0_types()),
        ]);
        let resolver = Resolver::new(cache);

        let layout = resolver.type_layout(type_("0xb0::m::T0")).await.unwrap();
        let expect = expect![[r#"
            struct 0xb0::m::T0 {
                m: struct 0xa0::m::T2 {
                    x: u8,
                },
                n: struct 0xa0::n::T0 {
                    t: struct 0xa0::m::T1<u16, u32> {
                        a: address,
                        p: u16,
                        q: vector<u32>,
                    },
                    u: struct 0xa0::m::T2 {
                        x: u8,
                    },
                },
            }"#]];

        expect.assert_eq(&format!("{layout:#}"));
    }

    /// A type from an upgraded package, mixing structs defined in the original package and the
    /// upgraded package.
    #[tokio::test]
    async fn test_upgraded_package() {
        let (_, cache) = package_cache([
            (1, build_package("a0"), a0_types()),
            (2, build_package("a1"), a1_types()),
        ]);
        let resolver = Resolver::new(cache);

        let layout = resolver.type_layout(type_("0xa1::n::T1")).await.unwrap();
        let expect = expect![[r#"
            struct 0xa1::n::T1 {
                t: struct 0xa0::m::T1<0xa1::m::T3, u32> {
                    a: address,
                    p: struct 0xa1::m::T3 {
                        y: u16,
                    },
                    q: vector<u32>,
                },
                u: struct 0xa1::m::T4 {
                    z: u32,
                },
            }"#]];

        expect.assert_eq(&format!("{layout:#}"));
    }

    /// A generic type instantiation where the type parameters are resolved relative to linkage
    /// contexts from different versions of the same package.
    #[tokio::test]
    async fn test_multiple_linkage_contexts() {
        let (_, cache) = package_cache([
            (1, build_package("a0"), a0_types()),
            (2, build_package("a1"), a1_types()),
        ]);
        let resolver = Resolver::new(cache);

        let layout = resolver
            .type_layout(type_("0xa0::m::T1<0xa0::m::T0, 0xa1::m::T3>"))
            .await
            .unwrap();

        let expect = expect![[r#"
            struct 0xa0::m::T1<0xa0::m::T0, 0xa1::m::T3> {
                a: address,
                p: struct 0xa0::m::T0 {
                    b: bool,
                    v: vector<struct 0xa0::m::T1<0xa0::m::T2, u128> {
                        a: address,
                        p: struct 0xa0::m::T2 {
                            x: u8,
                        },
                        q: vector<u128>,
                    }>,
                },
                q: vector<struct 0xa1::m::T3 {
                    y: u16,
                }>,
            }"#]];

        expect.assert_eq(&format!("{layout:#}"))
    }

    /// Refer to a type, not by its defining ID, but by the ID of some later version of that
    /// package.  This doesn't currently work during execution but it simplifies making queries: A
    /// type can be referred to using the ID of any package that declares it, rather than only the
    /// package that first declared it (whose ID is its defining ID).
    #[tokio::test]
    async fn test_upgraded_package_non_defining_id() {
        let (_, cache) = package_cache([
            (1, build_package("a0"), a0_types()),
            (2, build_package("a1"), a1_types()),
        ]);
        let resolver = Resolver::new(cache);

        let layout = resolver
            .type_layout(type_("0xa1::m::T1<0xa1::m::T3, 0xa1::m::T0>"))
            .await
            .unwrap();

        let expect = expect![[r#"
            struct 0xa0::m::T1<0xa1::m::T3, 0xa0::m::T0> {
                a: address,
                p: struct 0xa1::m::T3 {
                    y: u16,
                },
                q: vector<struct 0xa0::m::T0 {
                    b: bool,
                    v: vector<struct 0xa0::m::T1<0xa0::m::T2, u128> {
                        a: address,
                        p: struct 0xa0::m::T2 {
                            x: u8,
                        },
                        q: vector<u128>,
                    }>,
                }>,
            }"#]];

        expect.assert_eq(&format!("{layout:#}"));
    }

    /// A type that refers to a types in a relinked package.  C depends on B and overrides its
    /// dependency on A from v1 to v2.  The type in C refers to types that were defined in both B, A
    /// v1, and A v2.
    #[tokio::test]
    async fn test_relinking() {
        let (_, cache) = package_cache([
            (1, build_package("a0"), a0_types()),
            (2, build_package("a1"), a1_types()),
            (1, build_package("b0"), b0_types()),
            (1, build_package("c0"), c0_types()),
        ]);
        let resolver = Resolver::new(cache);

        let layout = resolver.type_layout(type_("0xc0::m::T0")).await.unwrap();
        let expect = expect![[r#"
            struct 0xc0::m::T0 {
                t: struct 0xa0::n::T0 {
                    t: struct 0xa0::m::T1<u16, u32> {
                        a: address,
                        p: u16,
                        q: vector<u32>,
                    },
                    u: struct 0xa0::m::T2 {
                        x: u8,
                    },
                },
                u: struct 0xa1::n::T1 {
                    t: struct 0xa0::m::T1<0xa1::m::T3, u32> {
                        a: address,
                        p: struct 0xa1::m::T3 {
                            y: u16,
                        },
                        q: vector<u32>,
                    },
                    u: struct 0xa1::m::T4 {
                        z: u32,
                    },
                },
                v: struct 0xa0::m::T2 {
                    x: u8,
                },
                w: struct 0xa1::m::T3 {
                    y: u16,
                },
                x: struct 0xb0::m::T0 {
                    m: struct 0xa0::m::T2 {
                        x: u8,
                    },
                    n: struct 0xa0::n::T0 {
                        t: struct 0xa0::m::T1<u16, u32> {
                            a: address,
                            p: u16,
                            q: vector<u32>,
                        },
                        u: struct 0xa0::m::T2 {
                            x: u8,
                        },
                    },
                },
            }"#]];

        expect.assert_eq(&format!("{layout:#}"));
    }

    #[tokio::test]
    async fn test_system_package_invalidation() {
        let (inner, cache) = package_cache([(1, build_package("s0"), s0_types())]);
        let resolver = Resolver::new(cache);

        let not_found = resolver.type_layout(type_("0x1::m::T1")).await.unwrap_err();
        assert!(matches!(not_found, Error::StructNotFound(_, _, _)));

        // Add a new version of the system package into the store underlying the cache.
        inner.write().unwrap().replace(
            addr("0x1"),
            cached_package(2, BTreeMap::new(), &build_package("s1"), &s1_types()),
        );

        let layout = resolver.type_layout(type_("0x1::m::T1")).await.unwrap();
        let expect = expect![[r#"
            struct 0x1::m::T1 {
                x: u256,
            }"#]];

        expect.assert_eq(&format!("{layout:#}"));
    }

    #[tokio::test]
    async fn test_caching() {
        let (inner, cache) = package_cache([
            (1, build_package("a0"), a0_types()),
            (1, build_package("s0"), s0_types()),
        ]);
        let resolver = Resolver::new(cache);

        let stats = |inner: &Arc<RwLock<InnerStore>>| {
            let i = inner.read().unwrap();
            (i.fetches, i.version_checks)
        };

        assert_eq!(stats(&inner), (0, 0));
        let l0 = resolver.type_layout(type_("0xa0::m::T0")).await.unwrap();

        // Load A0.
        assert_eq!(stats(&inner), (1, 0));

        // Layouts are the same, no need to reload the package.  Not a system package, so no version
        // check needed.
        let l1 = resolver.type_layout(type_("0xa0::m::T0")).await.unwrap();
        assert_eq!(format!("{l0}"), format!("{l1}"));
        assert_eq!(stats(&inner), (1, 0));

        // Different type, but same package, so no extra fetch.
        let l2 = resolver.type_layout(type_("0xa0::m::T2")).await.unwrap();
        assert_ne!(format!("{l0}"), format!("{l2}"));
        assert_eq!(stats(&inner), (1, 0));

        // New package to load.  It's a system package, which would need a version check if it
        // already existed in the cache, but it doesn't yet, so we only see a fetch.
        let l3 = resolver.type_layout(type_("0x1::m::T0")).await.unwrap();
        assert_eq!(stats(&inner), (2, 0));

        // Reload the same system package type, which will cause a version check.
        let l4 = resolver.type_layout(type_("0x1::m::T0")).await.unwrap();
        assert_eq!(format!("{l3}"), format!("{l4}"));
        assert_eq!(stats(&inner), (2, 1));

        // Upgrade the system package
        inner.write().unwrap().replace(
            addr("0x1"),
            cached_package(2, BTreeMap::new(), &build_package("s1"), &s1_types()),
        );

        // Reload the same system type again.  The version check fails and the system package is
        // refetched (even though the type is the same as before).  This usage pattern (layouts for
        // system types) is why a layout cache would be particularly helpful (future optimisation).
        let l5 = resolver.type_layout(type_("0x1::m::T0")).await.unwrap();
        assert_eq!(format!("{l4}"), format!("{l5}"));
        assert_eq!(stats(&inner), (3, 2));
    }

    #[tokio::test]
    async fn test_err_not_a_package() {
        let (_, cache) = package_cache([(1, build_package("a0"), a0_types())]);
        let resolver = Resolver::new(cache);
        let err = resolver
            .type_layout(type_("0x42::m::T0"))
            .await
            .unwrap_err();
        assert!(matches!(err, Error::PackageNotFound(_)));
    }

    #[tokio::test]
    async fn test_err_no_module() {
        let (_, cache) = package_cache([(1, build_package("a0"), a0_types())]);
        let resolver = Resolver::new(cache);
        let err = resolver
            .type_layout(type_("0xa0::l::T0"))
            .await
            .unwrap_err();
        assert!(matches!(err, Error::ModuleNotFound(_, _)));
    }

    #[tokio::test]
    async fn test_err_no_struct() {
        let (_, cache) = package_cache([(1, build_package("a0"), a0_types())]);
        let resolver = Resolver::new(cache);

        let err = resolver
            .type_layout(type_("0xa0::m::T9"))
            .await
            .unwrap_err();
        assert!(matches!(err, Error::StructNotFound(_, _, _)));
    }

    #[tokio::test]
    async fn test_err_type_arity() {
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
        let a0 = cache
            .fetch(AccountAddress::from_str("0xa0").unwrap())
            .await
            .unwrap();
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

        let expect = expect![[r#"
            a0::m::T0: StructDef {
                defining_id: 00000000000000000000000000000000000000000000000000000000000000a0,
                abilities: [],
                type_params: [],
                fields: [
                    (
                        "b",
                        Bool,
                    ),
                    (
                        "v",
                        Vector(
                            Struct(
                                StructRef {
                                    package: 00000000000000000000000000000000000000000000000000000000000000a0,
                                    module: "m",
                                    name: "T1",
                                },
                                [
                                    Struct(
                                        StructRef {
                                            package: 00000000000000000000000000000000000000000000000000000000000000a0,
                                            module: "m",
                                            name: "T2",
                                        },
                                        [],
                                    ),
                                    U128,
                                ],
                            ),
                        ),
                    ),
                ],
            }
            a0::m::T1: StructDef {
                defining_id: 00000000000000000000000000000000000000000000000000000000000000a0,
                abilities: [],
                type_params: [
                    StructTypeParameter {
                        constraints: [],
                        is_phantom: false,
                    },
                    StructTypeParameter {
                        constraints: [],
                        is_phantom: false,
                    },
                ],
                fields: [
                    (
                        "a",
                        Address,
                    ),
                    (
                        "p",
                        TypeParameter(
                            0,
                        ),
                    ),
                    (
                        "q",
                        Vector(
                            TypeParameter(
                                1,
                            ),
                        ),
                    ),
                ],
            }
            a0::m::T2: StructDef {
                defining_id: 00000000000000000000000000000000000000000000000000000000000000a0,
                abilities: [],
                type_params: [],
                fields: [
                    (
                        "x",
                        U8,
                    ),
                ],
            }"#]];
        expect.assert_eq(&format!(
            "a0::m::T0: {t0:#?}\n\
             a0::m::T1: {t1:#?}\n\
             a0::m::T2: {t2:#?}"
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

        let c0 = cache
            .fetch(AccountAddress::from_str("0xc0").unwrap())
            .await
            .unwrap();
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

        let expect = expect![[r#"
            c0::m::foo: FunctionDef {
                visibility: Public,
                is_entry: false,
                type_params: [],
                parameters: [],
                return_: [],
            }
            c0::m::bar: FunctionDef {
                visibility: Friend,
                is_entry: false,
                type_params: [],
                parameters: [
                    OpenSignature {
                        ref_: Some(
                            Immutable,
                        ),
                        body: Struct(
                            StructRef {
                                package: 00000000000000000000000000000000000000000000000000000000000000c0,
                                module: "m",
                                name: "T0",
                            },
                            [],
                        ),
                    },
                    OpenSignature {
                        ref_: Some(
                            Mutable,
                        ),
                        body: Struct(
                            StructRef {
                                package: 00000000000000000000000000000000000000000000000000000000000000a0,
                                module: "n",
                                name: "T1",
                            },
                            [],
                        ),
                    },
                ],
                return_: [
                    OpenSignature {
                        ref_: None,
                        body: U64,
                    },
                ],
            }
            c0::m::baz: FunctionDef {
                visibility: Private,
                is_entry: false,
                type_params: [],
                parameters: [
                    OpenSignature {
                        ref_: None,
                        body: U8,
                    },
                ],
                return_: [
                    OpenSignature {
                        ref_: None,
                        body: U16,
                    },
                    OpenSignature {
                        ref_: None,
                        body: U32,
                    },
                ],
            }"#]];
        expect.assert_eq(&format!(
            "c0::m::foo: {foo:#?}\n\
             c0::m::bar: {bar:#?}\n\
             c0::m::baz: {baz:#?}"
        ));
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

    /***** Test Helpers ***************************************************************************/

    type TypeOriginTable = Vec<StructKey>;

    fn a0_types() -> TypeOriginTable {
        vec![
            struct_("0xa0", "m", "T0"),
            struct_("0xa0", "m", "T1"),
            struct_("0xa0", "m", "T2"),
            struct_("0xa0", "n", "T0"),
        ]
    }

    fn a1_types() -> TypeOriginTable {
        let mut types = a0_types();

        types.extend([
            struct_("0xa1", "m", "T3"),
            struct_("0xa1", "m", "T4"),
            struct_("0xa1", "n", "T1"),
        ]);

        types
    }

    fn b0_types() -> TypeOriginTable {
        vec![struct_("0xb0", "m", "T0")]
    }

    fn c0_types() -> TypeOriginTable {
        vec![struct_("0xc0", "m", "T0")]
    }

    fn d0_types() -> TypeOriginTable {
        vec![
            struct_("0xd0", "m", "O"),
            struct_("0xd0", "m", "P"),
            struct_("0xd0", "m", "Q"),
            struct_("0xd0", "m", "R"),
            struct_("0xd0", "m", "S"),
            struct_("0xd0", "m", "T"),
        ]
    }

    fn s0_types() -> TypeOriginTable {
        vec![struct_("0x1", "m", "T0")]
    }

    fn s1_types() -> TypeOriginTable {
        let mut types = s0_types();

        types.extend([struct_("0x1", "m", "T1")]);

        types
    }

    fn sui_types() -> TypeOriginTable {
        vec![struct_("0x2", "object", "UID")]
    }

    /// Build an in-memory package cache from locally compiled packages.  Assumes that all packages
    /// in `packages` are published (all modules have a non-zero package address and all packages
    /// have a 'published-at' address), and their transitive dependencies are also in `packages`.
    fn package_cache(
        packages: impl IntoIterator<Item = (u64, CompiledPackage, TypeOriginTable)>,
    ) -> (Arc<RwLock<InnerStore>>, Box<dyn PackageStore>) {
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
            version_checks: 0,
        }));

        let store = InMemoryPackageStore {
            inner: inner.clone(),
        };

        let store_with_cache = PackageStoreWithLruCache::new(store);

        (inner, Box::new(store_with_cache))
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
        BuildConfig::new_for_testing().build(path).unwrap()
    }

    fn addr(a: &str) -> AccountAddress {
        AccountAddress::from_str(a).unwrap()
    }

    fn struct_(a: &str, m: &'static str, n: &'static str) -> StructKey {
        StructKey {
            package: addr(a),
            module: m.into(),
            name: n.into(),
        }
    }

    fn type_(t: &str) -> TypeTag {
        TypeTag::from_str(t).unwrap()
    }

    struct InMemoryPackageStore {
        /// All the contents are stored in an `InnerStore` that can be probed and queried from
        /// outside.
        inner: Arc<RwLock<InnerStore>>,
    }

    struct InnerStore {
        packages: BTreeMap<AccountAddress, Package>,
        fetches: usize,
        version_checks: usize,
    }

    #[async_trait]
    impl PackageStore for InMemoryPackageStore {
        async fn version(&self, id: AccountAddress) -> Result<SequenceNumber> {
            let mut inner = self.inner.as_ref().write().unwrap();
            inner.version_checks += 1;
            inner
                .packages
                .get(&id)
                .ok_or_else(|| Error::PackageNotFound(id))
                .map(|p| p.version)
        }

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
