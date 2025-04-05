// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
pub(crate) use checked::*;

#[sui_macros::with_checked_arithmetic]
mod checked {
    use move_binary_format::errors::{PartialVMError, PartialVMResult};
    use move_core_types::{
        account_address::AccountAddress,
        resolver::{ModuleResolver, SerializedPackage},
        vm_status::StatusCode,
    };
    use move_vm_runtime::shared::types::VersionId;
    use sui_types::{
        base_types::ObjectID, error::SuiResult, move_package::MovePackage,
        storage::BackingPackageStore,
    };

    // Implementation of the `DataStore` trait for the Move VM.
    // When used during execution it may have a list of new packages that have
    // just been published in the current context. Those are used for module/type
    // resolution when executing module init.
    // It may be created with an empty slice of packages either when no publish/upgrade
    // are performed or when a type is requested not during execution.
    pub(crate) struct SuiDataStore<'state, 'a> {
        /// Interface to resolve packages, modules and resources directly from the store.
        resolver: &'state dyn BackingPackageStore,
        /// New packages -- these are new package that have been published, and initialized
        /// successfully.
        new_packages: &'a [MovePackage],
        /// A possibly new package that we may be in the process of publishing and/or initializing.
        /// This package will only be added to `new_packages` iff publication and initialization
        /// (if any) is pefromed successfully.
        ephemeral_package: Option<&'a MovePackage>,
    }

    impl<'state, 'a> SuiDataStore<'state, 'a> {
        pub(crate) fn new(
            resolver: &'state dyn BackingPackageStore,
            new_packages: &'a [MovePackage],
        ) -> Self {
            Self {
                new_packages,
                resolver,
                ephemeral_package: None,
            }
        }

        pub(crate) fn new_with_ephemeral(
            resolver: &'state dyn BackingPackageStore,
            new_packages: &'a [MovePackage],
            ephemeral_package: Option<&'a MovePackage>,
        ) -> Self {
            Self {
                new_packages,
                resolver,
                ephemeral_package,
            }
        }

        fn get_package(&self, package_version_id: VersionId) -> Option<&MovePackage> {
            self.new_packages
                .iter()
                .chain(self.ephemeral_package.iter().cloned())
                .find(|package| *package.id() == package_version_id)
        }

        fn fetch_package(
            &self,
            package_version_id: VersionId,
        ) -> PartialVMResult<Option<SerializedPackage>> {
            Ok(match self.get_package(package_version_id) {
                Some(pkg) => Some(pkg.into_serialized_move_package()),
                None => {
                    match self
                        .resolver
                        .get_package_object(&ObjectID::from(package_version_id))
                    {
                        Ok(x) => x.map(|pkg| pkg.move_package().into_serialized_move_package()),
                        Err(err) => {
                            let msg = format!("Unexpected storage error: {:?}", err);
                            return Err(PartialVMError::new(
                                StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
                            )
                            .with_message(msg));
                        }
                    }
                }
            })
        }
    }

    // Better days have arrived!
    impl ModuleResolver for SuiDataStore<'_, '_> {
        type Error = PartialVMError;
        fn get_packages_static<const N: usize>(
            &self,
            ids: [AccountAddress; N],
        ) -> Result<[Option<SerializedPackage>; N], Self::Error> {
            // Once https://doc.rust-lang.org/stable/std/primitive.array.html#method.try_map is stable
            // we can use that here.
            let mut packages = [const { None }; N];
            for (i, id) in ids.iter().enumerate() {
                packages[i] = self.fetch_package(*id)?;
            }

            Ok(packages)
        }

        fn get_packages(
            &self,
            ids: &[AccountAddress],
        ) -> Result<Vec<Option<SerializedPackage>>, Self::Error> {
            ids.iter().map(|id| self.fetch_package(*id)).collect()
        }
    }

    // TODO(vm-rewrite): look at removing this in favor of jus using `SerializedPackage` once we
    // add the runtime ID to it.
    //
    // A unifying trait that allows us to load move packages, that may not be object just yet
    // (e.g., if they were published in the current transaction). Note that this needs to loade
    // `MovePackage`s and not just `SerializedPackage` as the version information contained in the
    // `MovePackage` is important to compute linkage. If we wanted to push this into the
    // `SerializedPackage` we could, and we could then remove this trait, however whether we want
    // to do that or not is a design decision that we should discuss.
    pub trait PackageStore {
        fn get_package(&self, id: &ObjectID) -> SuiResult<Option<MovePackage>>;
    }

    impl<T: BackingPackageStore> PackageStore for T {
        fn get_package(&self, id: &ObjectID) -> SuiResult<Option<MovePackage>> {
            Ok(self
                .get_package_object(id)?
                .map(|x| x.move_package().clone()))
        }
    }

    impl PackageStore for SuiDataStore<'_, '_> {
        fn get_package(&self, id: &ObjectID) -> SuiResult<Option<MovePackage>> {
            Ok(match self.get_package(**id) {
                Some(pkg) => Some(pkg.clone()),
                None => self
                    .resolver
                    .get_package_object(id)?
                    .map(|pkg| pkg.move_package().clone()),
            })
        }
    }
}
