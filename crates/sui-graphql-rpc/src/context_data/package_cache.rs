// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl};
use lru::LruCache;
use move_core_types::account_address::AccountAddress;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use sui_indexer::{indexer_reader::IndexerReader, schema_v2::objects};
use sui_package_resolver::{Error, Package, PackageStore, Result};
use sui_types::base_types::SequenceNumber;
use sui_types::is_system_package;
use sui_types::object::Object;

// TODO Move to ServiceConfig
const PACKAGE_CACHE_SIZE: NonZeroUsize = unsafe { NonZeroUsize::new_unchecked(1024) };

#[async_trait]
trait GraphqlPackageStore: PackageStore {
    async fn version(&self, id: AccountAddress) -> Result<SequenceNumber>;
}

/// Cache to answer queries that depend on information from move packages: listing a package's
/// modules, a module's structs and functions, the definitions or layouts of types, etc.
///
/// Queries that cannot be answered by the cache are served by loading the relevant package as an
/// object and parsing its contents.
pub struct PackageCache {
    packages: Mutex<LruCache<AccountAddress, Arc<Package>>>,
    store: Box<dyn GraphqlPackageStore + Send + Sync>,
}

#[async_trait]
impl PackageStore for PackageCache {
    async fn package(&self, id: AccountAddress) -> Result<Package> {
        self.package_impl(id).await.map(|p| (*p).clone())
    }
}

impl PackageCache {
    pub fn new(reader: IndexerReader) -> Self {
        Self::with_store(Box::new(DbPackageStore(reader)))
    }

    pub fn with_store(store: Box<dyn GraphqlPackageStore + Send + Sync>) -> Self {
        let packages = Mutex::new(LruCache::new(PACKAGE_CACHE_SIZE));
        Self { packages, store }
    }

    /// Return a deserialized representation of the package with ObjectID `id` on-chain.  Attempts
    /// to fetch this package from the cache, and if that fails, fetches it from the underlying data
    /// source and updates the cache.
    async fn package_impl(&self, id: AccountAddress) -> Result<Arc<Package>> {
        let candidate = {
            // Release the lock after getting the package
            let mut packages = self.packages.lock().unwrap();
            packages.get(&id).map(Arc::clone)
        };

        // System packages can be invalidated in the cache if a newer version exists.
        match candidate {
            Some(package) if !is_system_package(id) => return Ok(package),
            Some(package) if self.store.version(id).await? <= package.version() => {
                return Ok(package)
            }
            Some(_) | None => { /* nop */ }
        }

        let package = Arc::new(self.store.package(id).await?);

        // Try and insert the package into the cache, accounting for races.  In most cases the
        // racing fetches will produce the same package, but for system packages, they may not, so
        // favour the package that has the newer version, or if they are the same, the package that
        // is already in the cache.

        let mut packages = self.packages.lock().unwrap();
        Ok(match packages.peek(&id) {
            Some(prev) if package.version() <= prev.version() => {
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

struct DbPackageStore(IndexerReader);

#[async_trait]
impl GraphqlPackageStore for DbPackageStore {
    async fn version(&self, id: AccountAddress) -> Result<SequenceNumber> {
        let query = objects::dsl::objects
            .select(objects::dsl::object_version)
            .filter(objects::dsl::object_id.eq(id.to_vec()));

        let Some(version) = self
            .0
            .run_query_async(move |conn| query.get_result::<i64>(conn).optional())
            .await
            .map_err(|e| Error::PackageStoreError(Box::new(e)))?
        else {
            return Err(Error::PackageNotFound(id));
        };

        Ok(SequenceNumber::from_u64(version as u64))
    }
}

#[async_trait]
impl PackageStore for DbPackageStore {
    async fn package(&self, id: AccountAddress) -> Result<Package> {
        let query = objects::dsl::objects
            .select((
                objects::dsl::object_version,
                objects::dsl::serialized_object,
            ))
            .filter(objects::dsl::object_id.eq(id.to_vec()));

        let Some((version, bcs)) = self
            .0
            .run_query_async(move |conn| query.get_result::<(i64, Vec<u8>)>(conn).optional())
            .await
            .map_err(|e| Error::PackageStoreError(Box::new(e)))?
        else {
            return Err(Error::PackageNotFound(id));
        };

        let version = SequenceNumber::from_u64(version as u64);
        let object =
            bcs::from_bytes::<Object>(&bcs).map_err(|e| Error::PackageStoreError(Box::new(e)))?;

        Package::try_from_object(&object)
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use move_core_types::account_address::AccountAddress;
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::{path::PathBuf, str::FromStr, sync::RwLock};
    use sui_types::TypeTag;
    use super::PackageCache;

    use expect_test::expect;
    use move_compiler::compiled_unit::{CompiledUnitEnum, NamedCompiledModule};
    use sui_move_build::{BuildConfig, CompiledPackage};
    use sui_package_resolver::{Error, PackageStore};

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
    impl GraphqlPackageStore for InMemoryPackageStore {
        async fn version(&self, id: AccountAddress) -> Result<SequenceNumber> {
            let mut inner = self.inner.as_ref().write().unwrap();
            inner.version_checks += 1;
            inner
                .packages
                .get(&id)
                .ok_or_else(|| Error::PackageNotFound(id))
                .map(|p| p.version)
        }
    }

    #[async_trait]
    impl PackageStore for InMemoryPackageStore {
        async fn package(&self, id: AccountAddress) -> Result<Package> {
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
