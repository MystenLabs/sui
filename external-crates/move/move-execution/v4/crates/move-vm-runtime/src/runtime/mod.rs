// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::move_cache::{MoveCache, ResolvedPackageResult},
    dbg_println,
    execution::{dispatch_tables::VMDispatchTables, vm::MoveVM},
    jit,
    natives::{extensions::NativeExtensions, functions::NativeFunctions},
    runtime::telemetry::{MoveRuntimeTelemetry, TelemetryContext},
    shared::{
        gas::GasMeter,
        linkage_context::LinkageContext,
        system_packages::SystemPackages,
        types::{OriginalId, VersionId},
    },
    try_block,
    validation::{validate_for_publish, validate_for_vm_execution, verification::ast as verif_ast},
};

use move_binary_format::errors::VMResult;
use move_core_types::resolver::{ModuleResolver, SerializedPackage};
use move_vm_config::runtime::VMConfig;
use tracing::{error, instrument};

use std::{collections::BTreeMap, sync::Arc};

pub(crate) mod package_resolution;
pub mod telemetry;

#[allow(dead_code)]
#[derive(Debug)]
pub struct MoveRuntime {
    /// The VM package cache for the VM, holding currently-loaded packages.
    cache: Arc<MoveCache>,
    /// The native functions the Move VM uses
    natives: Arc<NativeFunctions>,
    /// The Move VM's configuration.
    vm_config: Arc<VMConfig>,
    /// Telemetry
    telemetry: Arc<TelemetryContext>,
}

impl MoveRuntime {
    pub fn new(natives: NativeFunctions, vm_config: VMConfig) -> Self {
        Self::new_with_system_packages(natives, vm_config, SystemPackages::empty())
    }

    pub fn new_with_default_config(natives: NativeFunctions) -> Self {
        Self::new(natives, VMConfig::default())
    }

    /// Construct a `MoveRuntime` with a set of pinned system packages installed at start-up.
    /// Each input package becomes identity-linked (`OriginalId == VersionId`) and stays alive
    /// for the lifetime of the runtime.
    ///
    /// **This call cannot fail.** Inputs that fail the identity-link check or the defining-ID
    /// check are dropped before being handed to the loader; per-package load/verify/JIT errors
    /// are logged and skipped. User packages that depend on a missing system package fall
    /// back to virtual-call dispatch.
    ///
    /// TODO: A richer cross-validation story across the input system packages (full transitive
    /// linkage validation among themselves) is left as future work; today each pkg is verified
    /// in isolation by the standard pipeline.
    pub fn new_with_system_packages(
        natives: NativeFunctions,
        vm_config: VMConfig,
        system_packages: SystemPackages,
    ) -> Self {
        let natives = Arc::new(natives);
        let vm_config = Arc::new(vm_config);
        let telemetry = Arc::new(TelemetryContext::new());
        let cache = Arc::new(MoveCache::new(vm_config.clone()));
        let mut runtime = Self {
            cache,
            natives,
            vm_config,
            telemetry,
        };
        runtime.install_system_packages(system_packages);
        runtime
    }

    /// Filter, validate, and install the input system packages into the cache. Run only at
    /// construction time, when this `MoveRuntime` holds the unique strong reference to its
    /// cache `Arc`.
    ///
    /// Filtering rejects two cheap-to-detect classes of bad input on the serialized form
    /// (logged + skipped, never fatal):
    ///   1. `original_id != version_id` — system packages must be identity-linked at v0.
    ///   2. Any `type_origin_table` entry whose defining id isn't `original_id` — the package's
    ///      types must originate from itself.
    ///
    /// Survivors are run through the standard load/verify/JIT pipeline one at a time so each
    /// install observes prior siblings already in `cache.system_packages` (the JIT translator
    /// snapshots that map once per `resolve_packages` call). Resolution failures and duplicate
    /// `OriginalId`s are logged; the runtime always constructs.
    fn install_system_packages(&mut self, system_packages: SystemPackages) {
        if system_packages.is_empty() {
            return;
        }

        let filtered: Vec<SerializedPackage> = system_packages
            .iter()
            .filter(|pkg| {
                if pkg.original_id != pkg.version_id {
                    error!(
                        version_id = %pkg.version_id,
                        original_id = %pkg.original_id,
                        "System package skipped: version_id != original_id (must be identity-linked)",
                    );
                    debug_assert!(
                        false,
                        "System package skipped: version_id != original_id (host bug)",
                    );
                    return false;
                }
                if let Some((name, defining_id)) = pkg
                    .type_origin_table
                    .iter()
                    .find(|(_, def_id)| **def_id != pkg.original_id)
                {
                    error!(
                        version_id = %pkg.version_id,
                        original_id = %pkg.original_id,
                        type_module = %name.module_name,
                        type_name = %name.type_name,
                        %defining_id,
                        "System package skipped: type defining_id does not match original_id",
                    );
                    debug_assert!(
                        false,
                        "System package skipped: type defining_id does not match original_id (host bug)",
                    );
                    return false;
                }
                true
            })
            .cloned()
            .collect();

        let deduped = SystemPackages::new(filtered);
        // Capture install order from the host-provided sequence *before* consuming into the
        // resolver, to ensure we load in the order the packages were provided (in order to handle
        // downstream dependencies correctly).
        let load_order_keys: Vec<VersionId> = deduped.iter().map(|p| p.version_id).collect();
        let resolver = deduped.into_resolver();

        // Split borrows of `self` so that the `&mut cache` borrow can coexist with the
        // immutable `&telemetry` / `&natives` borrows below.
        let Self {
            cache,
            telemetry,
            natives,
            ..
        } = self;
        let Some(cache) = Arc::get_mut(cache) else {
            error!(
                "install_system_packages: cache Arc is shared; system packages will not be installed",
            );
            // We just constructed this cache: `get_mut` must succeed. Missing it here means
            // we've handed out an Arc clone before install, which is an internal-invariant bug.
            debug_assert!(
                false,
                "install_system_packages: cache Arc must be unique at construction time",
            );
            return;
        };

        // Install each system package in its own `resolve_packages` call rather than in bulk:
        // the jit snapshots `cache.system_packages()` once per `resolve_packages` call, so a bulk
        // resolve would give every in-batch package a snapshot that omits its siblings, preventing
        // direct calls across pinned packages. Per-package installs make each successful install
        // visible to the next call's snapshot, letting later system packages direct-resolve into
        // earlier ones.
        //
        // Iterate `load_order_keys` (host-provided order) rather than iterating the resolver to
        // ensure expected load order.
        for version_id in load_order_keys {
            let result = telemetry.with_transaction_telemetry(|tx_telemetry| {
                package_resolution::resolve_packages(
                    &resolver,
                    tx_telemetry,
                    cache,
                    natives,
                    std::iter::once(version_id).collect(),
                )
            });
            match result {
                Ok(resolved) => {
                    for (_, p) in resolved {
                        let original_id = p.runtime.original_id;
                        if !cache.add_system_package(p) {
                            error!(
                                %original_id,
                                "System package already registered at original_id; skipping duplicate",
                            );
                            // Filter + `SystemPackages::new` dedup by version_id, and the filter
                            // enforces version_id == original_id, so duplicate original_ids
                            // should be impossible here.
                            debug_assert!(
                                false,
                                "duplicate system package original_id survived dedup + filter",
                            );
                        }
                    }
                }
                Err(err) => {
                    error!(
                        %version_id, error = ?err,
                        "System package install failed; skipping",
                    );
                    debug_assert!(
                        false,
                        "System package install failed for {version_id}: {err:?} (host bug)",
                    );
                }
            }
        }
    }

    /// Retrieive the Move VM Natives associated with the Runtime
    pub fn natives(&self) -> Arc<NativeFunctions> {
        self.natives.clone()
    }

    /// Retrieive the Move VM Config associated with the Runtime
    pub fn vm_config(&self) -> Arc<VMConfig> {
        self.vm_config.clone()
    }

    /// Retrieive the Move Cache associated with the Runtime
    pub fn cache(&self) -> Arc<MoveCache> {
        self.cache.clone()
    }

    /// Retrieive the Move Telemetry associated with the Runtime
    /// This may block if other threads are writing to the telemetry informtaion.
    pub fn get_telemetry_report(&self) -> MoveRuntimeTelemetry {
        self.telemetry.to_runtime_telemetry(&self.cache)
    }

    /// Resolve a package, loading it if necessary. This will use the provided `ModuleResolver` to
    /// fetch the package if it is not already cached.
    /// If there is an error loading or verifying the package, an error is returned.
    #[instrument(level = "trace", skip_all)]
    pub fn resolve_and_cache_package(
        &self,
        module_resolver: impl ModuleResolver,
        package_key: VersionId,
    ) -> VMResult<ResolvedPackageResult> {
        tracing::trace!(version_id = %package_key, "resolving and caching package");
        self.telemetry.with_transaction_telemetry(|txn_telemetry| {
            package_resolution::resolve_package(
                module_resolver,
                txn_telemetry,
                &self.cache,
                &self.natives,
                package_key,
            )
        })
    }

    /// Makes an Execution Instance for running a Move function invocation.
    /// Note this will hit the VM Cache to construct VTables for that execution, which may block on
    /// cache loading efforts.
    ///
    /// The resuling map of vtables _must_ be closed under the static dependency graph of the root
    /// package w.r.t, to the current linkage context in `data_store`.
    #[inline]
    #[instrument(level = "trace", skip_all)]
    pub fn make_vm<'extensions>(
        &self,
        package_store: impl ModuleResolver,
        link_context: LinkageContext,
    ) -> VMResult<MoveVM<'extensions>> {
        tracing::trace!(linkage_table = ?link_context, "making Move VM");
        self.make_vm_with_native_extensions(
            package_store,
            link_context,
            NativeExtensions::default(),
        )
    }

    #[instrument(level = "trace", skip_all)]
    pub fn make_vm_with_native_extensions<'extensions>(
        &self,
        package_store: impl ModuleResolver,
        link_context: LinkageContext,
        native_extensions: NativeExtensions<'extensions>,
    ) -> VMResult<MoveVM<'extensions>> {
        tracing::trace!(linkage_table = ?link_context, "making Move VM for execution with extensions");
        self.telemetry.with_transaction_telemetry(|txn_telemetry| {
            let total_timer = txn_telemetry.make_timer(crate::runtime::telemetry::TimerKind::Total);

            let instance = try_block! {
                let linkage_hash = link_context.to_linkage_hash();

                let mut virtual_tables = if let Some(vtables) =
                    self.cache.cached_linkage_tables_at(&linkage_hash)  {
                    vtables
                } else {
                    self.load_and_cache_vtables(
                        &package_store, txn_telemetry, &link_context, &linkage_hash
                    )?
                };

                // This is more a sanity check than anything else. The VMDispatchTables should
                // never have precomputed type depths, as those are computed on-demand.
                // If for some reason the cached VTables have precomputed type depths, or the
                // linkage context does not match the expected linkage context, then we drop the
                // cached VTables and reload them. This should never happen, but if it does, we
                // want to recover gracefully rather than erroring out with an invariant violation.
                if !virtual_tables.type_depths.is_empty() || link_context != *virtual_tables.link_context {
                    error!("Cached VTables for linkage context {:?} have precomputed type depths or do not match the expected linkage context. Dropping cached VTables and reloading.", link_context);
                    self.cache.drop_all_cached_linkage_tables();
                    virtual_tables = self.load_and_cache_vtables(
                        &package_store, txn_telemetry, &link_context, &linkage_hash
                    )?;
                }

                // Called and checked linkage, etc.
                let instance = MoveVM {
                    virtual_tables,
                    vm_config: self.vm_config.clone(),
                    interner: self.cache.interner.clone(),
                    link_context,
                    native_extensions: native_extensions.clone(),
                    telemetry: self.telemetry.clone(),
                };
                Ok(instance)
            };
            txn_telemetry.report_time(total_timer);
            instance
        })
    }

    /// Load and cache VTables for the provided linkage context
    /// .
    /// This will:
    /// - Load (or retrieve from the cache) all packages in the linkage context,
    /// - Perform cross-package verification,
    /// - Construct VTables for them,
    /// - Cache the VTables for future use,
    /// - and Return the VTables.
    ///
    /// If there is an error loading or verifying the packages, an error is returned instead.
    #[instrument(level = "trace", skip_all)]
    fn load_and_cache_vtables(
        &self,
        package_store: &impl ModuleResolver,
        txn_telemetry: &mut crate::runtime::telemetry::TransactionTelemetryContext,
        link_context: &LinkageContext,
        linkage_hash: &crate::shared::linkage_context::LinkageHash,
    ) -> Result<VMDispatchTables, move_binary_format::errors::VMError> {
        tracing::trace!(linkage_table = ?link_context, "loading and caching VTables for linkage context");
        let all_packages = link_context.all_packages()?;
        let packages = package_resolution::resolve_packages(
            package_store,
            txn_telemetry,
            &self.cache,
            &self.natives,
            all_packages,
        )?;
        let validation_packages = packages
            .iter()
            .map(|(id, pkg)| (*id, &*pkg.verified))
            .collect();
        validate_for_vm_execution(validation_packages, link_context)?;
        let runtime_packages = packages
            .into_values()
            .map(|pkg| (pkg.runtime.original_id, Arc::clone(&pkg.runtime)))
            .collect::<BTreeMap<OriginalId, Arc<jit::execution::ast::Package>>>();
        let vtables = VMDispatchTables::new(
            self.vm_config.clone(),
            self.cache.interner.clone(),
            link_context.clone(),
            runtime_packages,
        )?;
        self.cache
            .add_linkage_tables_to_cache(linkage_hash.clone(), vtables.clone());
        Ok(vtables)
    }

    /// Publish a package.
    ///
    /// This loads and validates the package against the VM cache and writes out publication
    /// effects to the provided data cache. The VM cache is not updated with the package, however.
    ///
    /// The Move VM MUST return a user error, i.e., an error that's not an invariant violation, if
    /// any module fails to deserialize or verify (see the full list of  failing conditions in the
    /// `publish_module` API). The publishing of the package is an all-or-nothing action: either
    /// all modules are published to the data store or none is.
    ///
    /// Similar to the `publish_module` API, the Move VM should not be able to produce other user
    /// errors. Besides, no user input should cause the Move VM to return an invariant violation.
    ///
    /// In case an invariant violation occurs, the provided data cache should be considered
    /// corrupted and discarded; a change set will not be returned.
    #[instrument(level = "trace", skip_all)]
    pub fn validate_package<'extensions>(
        &self,
        package_store: impl ModuleResolver,
        original_id: OriginalId,
        pkg: SerializedPackage,
        _gas_meter: &mut impl GasMeter,
        native_extensions: NativeExtensions<'extensions>,
    ) -> VMResult<(verif_ast::Package, MoveVM<'extensions>)> {
        tracing::trace!(
            version_id = %pkg.version_id,
            original_id = %original_id,
            version = %pkg.version,
            "validating package for publication and making VM instance",
        );
        let vm_telemetry = self.telemetry.clone();
        self.telemetry.with_transaction_telemetry(|txn_telemetry| {
            let total_timer = txn_telemetry.make_timer(crate::runtime::telemetry::TimerKind::Total);

            let result = try_block! {
                dbg_println!("\n\nPublishing module at {} (=> {original_id})\n\n", pkg.version_id);

                let link_context = LinkageContext::new(pkg.linkage_table.clone())?;

                // Verify a provided serialized package. This will validate the provided serialized
                // package, including attempting to jit-compile the package and verify linkage with
                // its dependencies in the provided linkage context. This returns the loaded
                // package in the case an `init` function or similar will need to run. This will
                // load the dependencies
                // into the package cache.
                let pkg_dependencies = package_resolution::resolve_packages(
                    package_store,
                    txn_telemetry,
                    &self.cache,
                    &self.natives,
                    link_context.all_package_dependencies_except(pkg.version_id)?,
                )?;
                let valdation_timer = txn_telemetry.make_timer_with_count(
                    crate::runtime::telemetry::TimerKind::Validation,
                    (pkg_dependencies.len() as u64).saturating_add(1),
                );
                let verified_pkg = {
                    let deps = pkg_dependencies
                        .iter()
                        .map(|(id, pkg)| (*id, &*pkg.verified))
                        .collect();
                    validate_for_publish(&self.natives, &self.vm_config, original_id, pkg, deps, &link_context)
                };
                txn_telemetry.report_time(valdation_timer);
                let verified_pkg = verified_pkg?;
                dbg_println!("\n\nVerified package\n\n");

                let published_package = package_resolution::jit_package_for_publish(
                    txn_telemetry,
                    &self.cache,
                    &self.natives,
                    self.cache.system_packages(),
                    verified_pkg.clone(),
                )?;

                // Generates  a one-off package for executing `init` functions.
                let runtime_packages = pkg_dependencies
                    .into_values()
                    .chain([published_package])
                    .map(|pkg| (pkg.runtime.original_id, Arc::clone(&pkg.runtime)))
                    .collect::<BTreeMap<OriginalId, Arc<jit::execution::ast::Package>>>();

                let virtual_tables = VMDispatchTables::new(
                    self.vm_config.clone(),
                    self.cache.interner.clone(),
                    link_context.clone(),
                    runtime_packages,
                )?;

                // Called and checked linkage, etc.
                let instance = MoveVM {
                    virtual_tables,
                    telemetry: vm_telemetry,
                    vm_config: self.vm_config.clone(),
                    interner: self.cache.interner.clone(),
                    link_context,
                    native_extensions: native_extensions.clone(),
                };

                Ok((verified_pkg, instance))
            };
            txn_telemetry.report_time(total_timer);
            result
        })
    }
}
