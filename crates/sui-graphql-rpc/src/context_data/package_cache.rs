// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    borrow::Cow,
    collections::BTreeMap,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl};
use lru::LruCache;
use move_binary_format::{
    access::ModuleAccess,
    errors::{Location, VMError},
    file_format::{
        SignatureToken, StructDefinitionIndex, StructFieldInformation, StructHandleIndex,
        TableIndex,
    },
    CompiledModule,
};
use move_core_types::{
    account_address::AccountAddress,
    language_storage::{StructTag, TypeTag},
    value::{MoveFieldLayout, MoveStructLayout, MoveTypeLayout},
};
use sui_indexer::{
    errors::IndexerError as DbError, indexer_reader::IndexerReader, schema_v2::objects,
};
use sui_types::{
    base_types::SequenceNumber, is_system_package, move_package::TypeOrigin, object::Object,
    Identifier,
};
use thiserror::Error;

// TODO Move to ServiceConfig
const PACKAGE_CACHE_SIZE: NonZeroUsize = unsafe { NonZeroUsize::new_unchecked(1024) };

/// Cache to answer queries that depend on information from move packages: listing a package's
/// modules, a module's structs and functions, the definitions or layouts of types, etc.
///
/// Queries that cannot be answered by the cache are served by loading the relevant package as an
/// object and parsing its contents.
pub(crate) struct PackageCache {
    packages: Mutex<LruCache<AccountAddress, Arc<Package>>>,
    store: Box<dyn PackageStore + Send + Sync>,
}

struct DbPackageStore(IndexerReader);

#[derive(Error, Debug)]
pub(crate) enum Error {
    #[error("{0}")]
    Bcs(#[from] bcs::Error),

    #[error("{0}")]
    Db(#[from] DbError),

    #[error("{0}")]
    Deserialize(VMError),

    #[error("Package has no modules: {0}")]
    EmptyPackage(AccountAddress),

    #[error("Linkage not found for package: {0}")]
    LinkageNotFound(AccountAddress),

    #[error("Module not found: {0}::{1}")]
    ModuleNotFound(AccountAddress, String),

    #[error("No origin package found for {0}::{1}::{2}")]
    NoTypeOrigin(AccountAddress, String, String),

    #[error("Not a package: {0}")]
    NotAPackage(AccountAddress),

    #[error("Not an identifier: '{0}'")]
    NotAnIdentifier(String),

    #[error("Package not found: {0}")]
    PackageNotFound(AccountAddress),

    #[error("Struct not found: {0}::{1}::{2}")]
    StructNotFound(AccountAddress, String, String),

    #[error("Expected {0} type parameters, but got {1}")]
    TypeArityMismatch(u16, usize),

    #[error("Type Parameter {0} out of bounds ({1})")]
    TypeParamOOB(u16, usize),

    #[error("Unexpected reference type.")]
    UnexpectedReference,

    #[error("Unexpected type: 'signer'.")]
    UnexpectedSigner,
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug)]
struct Package {
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
struct Module {
    bytecode: CompiledModule,

    /// Index mapping struct names to their defining ID, and the index for their definition in the
    /// bytecode, to speed up definition lookups.
    struct_index: BTreeMap<String, (AccountAddress, StructDefinitionIndex)>,
}

/// Deserialized representation of a struct definition.
#[derive(Debug)]
struct StructDef {
    /// The storage ID of the package that first introduced this type.
    defining_id: AccountAddress,

    /// Number of type parameters.
    type_params: u16,

    /// Serialized representation of fields (names and deserialized signatures). Signatures refer to
    /// packages at their runtime IDs (not their storage ID or defining ID).
    fields: Vec<(String, OpenSignature)>,
}

/// Fully qualified struct identifier.  Uses copy-on-write strings so that when it is used as a key
/// to a map, an instance can be created to query the map without having to allocate strings on the
/// heap.
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone, Hash)]
struct StructRef<'m, 'n> {
    package: AccountAddress,
    module: Cow<'m, str>,
    name: Cow<'n, str>,
}

/// A `StructRef` that owns its strings.
type StructKey = StructRef<'static, 'static>;

/// Deserialized representation of a type signature that could appear as a field type for a struct.
/// Signatures refer to structs at their runtime IDs and can contain references to free type
/// parameters but will not contain reference types.
#[derive(Clone, Debug)]
enum OpenSignature {
    Address,
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
    Vector(Box<OpenSignature>),
    Struct(StructKey, Vec<OpenSignature>),
    TypeParameter(u16),
}

/// Information necessary to convert a type tag into a type layout.
#[derive(Debug, Default)]
struct ResolutionContext {
    /// Definitions (field information) for structs referred to by types added to this context.
    structs: BTreeMap<StructKey, StructDef>,
}

/// Interface to abstract over access to a store of live packages.  Used to override the default
/// store during testing.
#[async_trait]
trait PackageStore {
    /// Latest version of the object at `id`.
    async fn version(&self, id: AccountAddress) -> Result<SequenceNumber>;

    /// Read package contents.  Fails if `id` is not an object, not a package, or is malformed in
    /// some way.
    async fn fetch(&self, id: AccountAddress) -> Result<Package>;
}

impl PackageCache {
    pub(crate) fn new(reader: IndexerReader) -> Self {
        Self::with_store(Box::new(DbPackageStore(reader)))
    }

    fn with_store(store: Box<dyn PackageStore + Send + Sync>) -> Self {
        let packages = Mutex::new(LruCache::new(PACKAGE_CACHE_SIZE));
        Self { packages, store }
    }

    /// Return the type layout corresponding to the given type tag.  The layout always refers to
    /// structs in terms of their defining ID (i.e. their package ID always points to the first
    /// package that introduced them).
    pub(crate) async fn type_layout(&self, mut tag: TypeTag) -> Result<MoveTypeLayout> {
        let mut context = ResolutionContext::default();

        // (1). Fetch all the information from this cache that is necessary to resolve types
        // referenced by this tag.
        context.add_type_tag(&mut tag, self).await?;

        // (2). Use that information to resolve the tag into a layout.
        context.resolve_type_tag(&tag)
    }

    /// Return a deserialized representation of the package with ObjectID `id` on-chain.  Attempts
    /// to fetch this package from the cache, and if that fails, fetches it from the underlying data
    /// source and updates the cache.
    async fn package(&self, id: AccountAddress) -> Result<Arc<Package>> {
        let candidate = {
            // Release the lock after getting the package
            let mut packages = self.packages.lock().unwrap();
            packages.get(&id).map(Arc::clone)
        };

        // System packages can be invalidated in the cache if a newer version exists.
        match candidate {
            Some(package) if !is_system_package(id) => return Ok(package),
            Some(package) if self.store.version(id).await? <= package.version => {
                return Ok(package)
            }
            Some(_) | None => { /* nop */ }
        }

        let package = Arc::new(self.store.fetch(id).await?);

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
    fn module(&self, module: &str) -> Result<&Module> {
        self.modules
            .get(module)
            .ok_or_else(|| Error::ModuleNotFound(self.storage_id, module.to_string()))
    }

    fn struct_def(&self, module_name: &str, struct_name: &str) -> Result<StructDef> {
        let module = self.module(module_name)?;
        let Some(&(defining_id, index)) = module.struct_index.get(struct_name) else {
            return Err(Error::StructNotFound(
                self.storage_id,
                module_name.to_string(),
                struct_name.to_string(),
            ));
        };

        let struct_def = module.bytecode.struct_def_at(index);
        let struct_handle = module.bytecode.struct_handle_at(struct_def.struct_handle);
        let type_params = struct_handle.type_parameters.len() as u16;

        let fields = match &struct_def.field_information {
            StructFieldInformation::Native => vec![],
            StructFieldInformation::Declared(fields) => fields
                .iter()
                .map(|f| {
                    Ok((
                        module.bytecode.identifier_at(f.name).to_string(),
                        OpenSignature::read(&f.signature.0, &module.bytecode)?,
                    ))
                })
                .collect::<Result<_>>()?,
        };

        Ok(StructDef {
            defining_id,
            type_params,
            fields,
        })
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

        Ok(Module {
            bytecode,
            struct_index,
        })
    }
}

impl OpenSignature {
    fn read(sig: &SignatureToken, bytecode: &CompiledModule) -> Result<Self> {
        use OpenSignature as O;
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

            S::Vector(sig) => O::Vector(Box::new(OpenSignature::read(sig, bytecode)?)),

            S::Struct(ix) => O::Struct(StructKey::read(*ix, bytecode), vec![]),
            S::StructInstantiation(ix, params) => O::Struct(
                StructKey::read(*ix, bytecode),
                params
                    .iter()
                    .map(|sig| OpenSignature::read(sig, bytecode))
                    .collect::<Result<_>>()?,
            ),
        })
    }
}

impl<'m, 'n> StructRef<'m, 'n> {
    fn as_key(&self) -> StructKey {
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

impl ResolutionContext {
    /// Add all the necessary information to resolve `tag` into this resolution context, fetching
    /// data from `cache` as necessary. Also updates package addresses in `tag` to point to runtime
    /// IDs instead of storage IDs to ensure queries made using these addresses during the
    /// resolution phase find the relevant field information in the context.
    async fn add_type_tag(&mut self, tag: &mut TypeTag, cache: &PackageCache) -> Result<()> {
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
                    let context = cache.package(s.address).await?;
                    let struct_def = context.struct_def(s.module.as_str(), s.name.as_str())?;

                    // Normalize `address` (the ID of a package that contains the definition of this
                    // struct) to be a runtime ID, because that's what the resolution context uses
                    // for keys.  Take care to do this before generating the key that is used to
                    // query and/or write into `self.structs.
                    s.address = context.runtime_id;
                    let key = StructRef::from(s.as_ref()).as_key();

                    frontier.extend(s.type_params.iter_mut());

                    if self.structs.contains_key(&key) {
                        continue;
                    }

                    for (_, sig) in &struct_def.fields {
                        self.add_signature(sig.clone(), cache, &context).await?;
                    }

                    self.structs.insert(key, struct_def);
                }
            }
        }

        Ok(())
    }

    // Like `add_type_tag` but for type signatures.  Needs a linkage table to translate runtime IDs
    // into storage IDs.
    async fn add_signature(
        &mut self,
        sig: OpenSignature,
        cache: &PackageCache,
        context: &Package,
    ) -> Result<()> {
        use OpenSignature as O;

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
                    frontier.extend(params.into_iter());

                    if self.structs.contains_key(&key) {
                        continue;
                    }

                    let storage_id = context.relocate(key.package)?;
                    let package = cache.package(storage_id).await?;
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
    fn resolve_type_tag(&self, tag: &TypeTag) -> Result<MoveTypeLayout> {
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

            T::Vector(tag) => L::Vector(Box::new(self.resolve_type_tag(tag)?)),

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

                if def.type_params as usize != type_params.len() {
                    return Err(Error::TypeArityMismatch(def.type_params, type_params.len()));
                }

                // TODO (optimization): This could be made more efficient by only generating layouts
                // for non-phantom types.  This efficiency could be extended to the exploration
                // phase (i.e. only explore layouts of non-phantom types). But this optimisation is
                // complicated by the fact that we still need to create a correct type tag for a
                // phantom parameter, which is currently done by converting a type layout into a
                // tag.
                let param_layouts = type_params
                    .iter()
                    .map(|tag| self.resolve_type_tag(tag))
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

                L::Struct(MoveStructLayout::WithTypes { type_, fields })
            }
        })
    }

    /// Like `resolve_type_tag` but for signatures.  Needs to be provided the layouts of type
    /// parameters which are substituted when a type parameter is encountered.
    fn resolve_signature(
        &self,
        sig: &OpenSignature,
        param_layouts: &Vec<MoveTypeLayout>,
    ) -> Result<MoveTypeLayout> {
        use MoveTypeLayout as L;
        use OpenSignature as O;

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
                assert!(self.structs.contains_key(key), "Missing: {key:#?}");

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

                L::Struct(MoveStructLayout::WithTypes { type_, fields })
            }
        })
    }
}

#[async_trait]
impl PackageStore for DbPackageStore {
    async fn version(&self, id: AccountAddress) -> Result<SequenceNumber> {
        let query = objects::dsl::objects
            .select(objects::dsl::object_version)
            .filter(objects::dsl::object_id.eq(id.to_vec()));

        let Some(version) = self
            .0
            .run_query_async(move |conn| query.get_result::<i64>(conn).optional())
            .await?
        else {
            return Err(Error::PackageNotFound(id));
        };

        Ok(SequenceNumber::from_u64(version as u64))
    }

    async fn fetch(&self, id: AccountAddress) -> Result<Package> {
        let query = objects::dsl::objects
            .select((
                objects::dsl::object_version,
                objects::dsl::serialized_object,
            ))
            .filter(objects::dsl::object_id.eq(id.to_vec()));

        let Some((version, bcs)) = self
            .0
            .run_query_async(move |conn| query.get_result::<(i64, Vec<u8>)>(conn).optional())
            .await?
        else {
            return Err(Error::PackageNotFound(id));
        };

        let version = SequenceNumber::from_u64(version as u64);
        let object = bcs::from_bytes::<Object>(&bcs)?;

        let Some(package) = object.data.try_as_package() else {
            return Err(Error::NotAPackage(id));
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
                Err(struct_) => return Err(Error::NoTypeOrigin(id, name, struct_)),
            };
        }

        let Some(runtime_id) = runtime_id else {
            return Err(Error::EmptyPackage(id));
        };

        let linkage = package
            .linkage_table()
            .iter()
            .map(|(&dep, linkage)| (dep.into(), linkage.upgraded_id.into()))
            .collect();

        Ok(Package {
            storage_id: id,
            runtime_id,
            version,
            modules,
            linkage,
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

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, str::FromStr, sync::RwLock};

    use expect_test::expect;
    use move_compiler::compiled_unit::{CompiledUnitEnum, NamedCompiledModule};
    use sui_move_build::{BuildConfig, CompiledPackage};

    use super::*;

    /// Layout for a type that only refers to base types or other types in the same module.
    #[tokio::test]
    async fn test_simple_type() {
        let (_, cache) = package_cache([(1, build_package("a0"), a0_types())]);

        let layout = cache.type_layout(type_("0xa0::m::T0")).await.unwrap();
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

        let layout = cache.type_layout(type_("0xa0::n::T0")).await.unwrap();
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

        let layout = cache.type_layout(type_("0xb0::m::T0")).await.unwrap();
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

        let layout = cache.type_layout(type_("0xa1::n::T1")).await.unwrap();
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

        let layout = cache
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

        let layout = cache
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

        let layout = cache.type_layout(type_("0xc0::m::T0")).await.unwrap();
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

        let not_found = cache.type_layout(type_("0x1::m::T1")).await.unwrap_err();
        assert!(matches!(not_found, Error::StructNotFound(_, _, _)));

        // Add a new version of the system package into the store underlying the cache.
        inner.write().unwrap().replace(
            addr("0x1"),
            cached_package(2, BTreeMap::new(), &build_package("s1"), &s1_types()),
        );

        let layout = cache.type_layout(type_("0x1::m::T1")).await.unwrap();
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

        let stats = |inner: &Arc<RwLock<InnerStore>>| {
            let i = inner.read().unwrap();
            (i.fetches, i.version_checks)
        };

        assert_eq!(stats(&inner), (0, 0));
        let l0 = cache.type_layout(type_("0xa0::m::T0")).await.unwrap();

        // Load A0.
        assert_eq!(stats(&inner), (1, 0));

        // Layouts are the same, no need to reload the package.  Not a system package, so no version
        // check needed.
        let l1 = cache.type_layout(type_("0xa0::m::T0")).await.unwrap();
        assert_eq!(format!("{l0}"), format!("{l1}"));
        assert_eq!(stats(&inner), (1, 0));

        // Different type, but same package, so no extra fetch.
        let l2 = cache.type_layout(type_("0xa0::m::T2")).await.unwrap();
        assert_ne!(format!("{l0}"), format!("{l2}"));
        assert_eq!(stats(&inner), (1, 0));

        // New package to load.  It's a system package, which would need a version check if it
        // already existed in the cache, but it doesn't yet, so we only see a fetch.
        let l3 = cache.type_layout(type_("0x1::m::T0")).await.unwrap();
        assert_eq!(stats(&inner), (2, 0));

        // Reload the same system package type, which will cause a version check.
        let l4 = cache.type_layout(type_("0x1::m::T0")).await.unwrap();
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
        let l5 = cache.type_layout(type_("0x1::m::T0")).await.unwrap();
        assert_eq!(format!("{l4}"), format!("{l5}"));
        assert_eq!(stats(&inner), (3, 2));
    }

    #[tokio::test]
    async fn test_err_not_a_package() {
        let (_, cache) = package_cache([(1, build_package("a0"), a0_types())]);
        let err = cache.type_layout(type_("0x42::m::T0")).await.unwrap_err();
        assert!(matches!(err, Error::PackageNotFound(_)));
    }

    #[tokio::test]
    async fn test_err_no_module() {
        let (_, cache) = package_cache([(1, build_package("a0"), a0_types())]);
        let err = cache.type_layout(type_("0xa0::l::T0")).await.unwrap_err();
        assert!(matches!(err, Error::ModuleNotFound(_, _)));
    }

    #[tokio::test]
    async fn test_err_no_struct() {
        let (_, cache) = package_cache([(1, build_package("a0"), a0_types())]);
        let err = cache.type_layout(type_("0xa0::m::T9")).await.unwrap_err();
        assert!(matches!(err, Error::StructNotFound(_, _, _)));
    }

    #[tokio::test]
    async fn test_err_type_arity() {
        let (_, cache) = package_cache([(1, build_package("a0"), a0_types())]);

        // Too few
        let err = cache
            .type_layout(type_("0xa0::m::T1<u8>"))
            .await
            .unwrap_err();
        assert!(matches!(err, Error::TypeArityMismatch(2, 1)));

        // Too many
        let err = cache
            .type_layout(type_("0xa0::m::T1<u8, u16, u32>"))
            .await
            .unwrap_err();
        assert!(matches!(err, Error::TypeArityMismatch(2, 3)));
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

    fn s0_types() -> TypeOriginTable {
        vec![struct_("0x1", "m", "T0")]
    }

    fn s1_types() -> TypeOriginTable {
        let mut types = s0_types();

        types.extend([struct_("0x1", "m", "T1")]);

        types
    }

    /// Build an in-memory package cache from locally compiled packages.  Assumes that all packages
    /// in `packages` are published (all modules have a non-zero package address and all packages
    /// have a 'published-at' address), and their transitive dependencies are also in `packages`.
    fn package_cache(
        packages: impl IntoIterator<Item = (u64, CompiledPackage, TypeOriginTable)>,
    ) -> (Arc<RwLock<InnerStore>>, PackageCache) {
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

        (inner, PackageCache::with_store(Box::new(store)))
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
            let CompiledUnitEnum::Module(NamedCompiledModule { name, module, .. }) = &unit.unit
            else {
                panic!("Modules only -- no script allowed.");
            };

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

        async fn fetch(&self, id: AccountAddress) -> Result<Package> {
            let mut inner = self.inner.as_ref().write().unwrap();
            inner.fetches += 1;
            inner
                .packages
                .get(&id)
                .cloned()
                .ok_or_else(|| Error::PackageNotFound(id))
        }
    }

    impl InnerStore {
        fn replace(&mut self, id: AccountAddress, package: Package) {
            self.packages.insert(id, package);
        }
    }
}
