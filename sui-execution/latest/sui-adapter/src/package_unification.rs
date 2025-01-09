use std::collections::{BTreeMap, BTreeSet};

use move_binary_format::{binary_config::BinaryConfig, file_format::Visibility};
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    move_package::{normalize_module, MovePackage},
    storage::BackingPackageStore,
    transaction::{Command, ProgrammableTransaction},
    type_input::TypeInput,
    Identifier,
};

pub struct PTBLinkageMetadata {
    pub types: Vec<ObjectID>,
    pub entry_functions: Vec<ObjectID>,
    pub non_entry_functions: Vec<ObjectID>,
    pub publication_linkages: Vec<ObjectID>,
    pub upgrading_packages: Vec<ObjectID>,
    pub all_packages: BTreeMap<ObjectID, MovePackage>,
}

pub struct LinkageConfig {
    pub fix_top_level_functions: bool,
    pub fix_types: bool,
}

#[derive(Debug)]
pub enum ConflictResolution {
    Exact(SequenceNumber, ObjectID),
    AtLeast(SequenceNumber, ObjectID),
    Never(ObjectID),
}

impl LinkageConfig {
    pub fn strict() -> Self {
        Self {
            fix_top_level_functions: true,
            fix_types: true,
        }
    }

    pub fn loose() -> Self {
        Self {
            fix_top_level_functions: false,
            fix_types: false,
        }
    }
}

impl ConflictResolution {
    pub fn unify(&mut self, other: &ConflictResolution) -> anyhow::Result<()> {
        match (&self, other) {
            // If we ever try to unify with a Never we fail.
            (ConflictResolution::Never(_), _) | (_, ConflictResolution::Never(_)) => {
                return Err(anyhow::anyhow!(
                    "Cannot unify with Never: {:?} and {:?}",
                    self,
                    other
                ));
            }
            // If we have two exact resolutions, they must be the same.
            (ConflictResolution::Exact(sv, self_id), ConflictResolution::Exact(ov, other_id)) => {
                if self_id != other_id {
                    return Err(anyhow::anyhow!(
                        "Exact/exact conflicting resolutions for packages: {self_id}@{sv} and {other_id}@{ov}",
                    ));
                }
            }
            // Take the max if you have two at least resolutions.
            (
                ConflictResolution::AtLeast(self_version, sid),
                ConflictResolution::AtLeast(other_version, oid),
            ) => {
                let id = if self_version > other_version {
                    *sid
                } else {
                    *oid
                };

                *self = ConflictResolution::AtLeast(*self_version.max(other_version), id);
            }
            // If you unify an exact and an at least, the exact must be greater than or equal to
            // the at least. It unifies to an exact.
            (
                ConflictResolution::Exact(exact_version, self_id),
                ConflictResolution::AtLeast(at_least_version, oid),
            )
            | (
                ConflictResolution::AtLeast(at_least_version, oid),
                ConflictResolution::Exact(exact_version, self_id),
            ) => {
                if exact_version < at_least_version {
                    return Err(anyhow::anyhow!(
                        "Exact/at least Conflicting resolutions for packages: Exact {self_id}@{exact_version} and {oid}@{at_least_version}",
                    ));
                }

                *self = ConflictResolution::Exact(*exact_version, *self_id);
            }
        }

        Ok(())
    }
}

impl PTBLinkageMetadata {
    pub fn from_ptb(
        ptb: &ProgrammableTransaction,
        store: &dyn BackingPackageStore,
    ) -> anyhow::Result<Self> {
        let mut linkage = PTBLinkageMetadata {
            types: Vec::new(),
            entry_functions: Vec::new(),
            non_entry_functions: Vec::new(),
            publication_linkages: Vec::new(),
            upgrading_packages: Vec::new(),
            all_packages: BTreeMap::new(),
        };

        for command in &ptb.commands {
            linkage.add_command(command, store)?;
        }

        Ok(linkage)
    }

    fn add_command(
        &mut self,
        command: &Command,
        store: &dyn BackingPackageStore,
    ) -> anyhow::Result<()> {
        match command {
            Command::MoveCall(programmable_move_call) => {
                let pkg = self.get_package(&programmable_move_call.package, store)?;

                // TODO/XXX: Make this work without needing to normalize the module.
                let module = normalize_module(
                    pkg.serialized_module_map()
                        .get(&programmable_move_call.module)
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "Module {} not found in package {}",
                                programmable_move_call.module,
                                pkg.id()
                            )
                        })?,
                    &BinaryConfig::standard(),
                )?;
                let function = module
                    .functions
                    .get(&Identifier::new(programmable_move_call.function.clone())?)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Function {} not found in module {}",
                            programmable_move_call.function,
                            module.module_id(),
                        )
                    })?;

                let pkg_id = pkg.id();
                let transitive_deps = pkg
                    .linkage_table()
                    .values()
                    .map(|info| info.upgraded_id)
                    .collect::<Vec<_>>();

                // load transitive deps
                for id in transitive_deps {
                    self.get_package(&id, store)?;
                }

                // Register function entrypoint
                if function.is_entry && function.visibility != Visibility::Public {
                    self.entry_functions.push(pkg_id);
                } else {
                    self.non_entry_functions.push(pkg_id);
                }

                for ty in &programmable_move_call.type_arguments {
                    self.add_type(ty, store)?;
                }
            }
            Command::MakeMoveVec(type_input, _) => {
                if let Some(ty) = type_input {
                    self.add_type(ty, store)?;
                }
            }
            Command::Upgrade(_, transitive_deps, upgrading_object_id, _) => {
                self.upgrading_packages.push(*upgrading_object_id);
                self.get_package(upgrading_object_id, store)?;

                self.publication_linkages.extend_from_slice(transitive_deps);
                for object_id in transitive_deps {
                    self.get_package(object_id, store)?;
                }
            }
            Command::Publish(_, transitive_deps) => {
                self.publication_linkages.extend_from_slice(transitive_deps);
                for object_id in transitive_deps {
                    self.get_package(object_id, store)?;
                }
            }
            Command::TransferObjects(_, _) => (),
            Command::SplitCoins(_, _) => (),
            Command::MergeCoins(_, _) => (),
        };

        Ok(())
    }

    fn add_type(&mut self, ty: &TypeInput, store: &dyn BackingPackageStore) -> anyhow::Result<()> {
        let mut stack = vec![ty];
        while let Some(ty) = stack.pop() {
            match ty {
                TypeInput::Bool
                | TypeInput::U8
                | TypeInput::U64
                | TypeInput::U128
                | TypeInput::Address
                | TypeInput::Signer
                | TypeInput::U16
                | TypeInput::U32
                | TypeInput::U256 => (),
                TypeInput::Vector(type_input) => {
                    stack.push(&**type_input);
                }
                TypeInput::Struct(struct_input) => {
                    let pkg = self
                        .get_package(&ObjectID::from(struct_input.address), store)?
                        .id();
                    self.types.push(pkg);
                    for ty in struct_input.type_params.iter() {
                        stack.push(ty);
                    }
                }
            }
        }
        Ok(())
    }

    // Gather and dedup all packages loaded by the PTB.
    // Also gathers the versions of each package loaded by the PTB at the same time.
    fn get_package(
        &mut self,
        object_id: &ObjectID,
        store: &dyn BackingPackageStore,
    ) -> anyhow::Result<&MovePackage> {
        if !self.all_packages.contains_key(object_id) {
            let package = store
                .get_package_object(object_id)?
                .ok_or_else(|| anyhow::anyhow!("Object {} not found in any package", object_id))?
                .move_package()
                .clone();
            self.all_packages.insert(*object_id, package);
        }

        Ok(self
            .all_packages
            .get(object_id)
            .expect("Guaranteed to exist"))
    }
}

impl PTBLinkageMetadata {
    pub fn try_compute_unified_linkage(
        &self,
        linking_config: LinkageConfig,
    ) -> anyhow::Result<BTreeSet<ObjectID>> {
        let mut unification_table = BTreeMap::new();

        // Any packages that are being upgraded cannot be called in the transaction
        for object_id in self.upgrading_packages.iter() {
            Self::add_and_unify(
                &mut unification_table,
                &self.all_packages,
                object_id,
                |pkg| ConflictResolution::Never(pkg.id()),
            )?;
        }

        // Linkages for packages that are to be published must be exact.
        for object_id in self.publication_linkages.iter() {
            Self::add_and_unify(
                &mut unification_table,
                &self.all_packages,
                object_id,
                |pkg| ConflictResolution::Exact(pkg.version(), pkg.id()),
            )?;
        }

        for object_id in self.entry_functions.iter() {
            Self::add_and_unify(
                &mut unification_table,
                &self.all_packages,
                object_id,
                |pkg| ConflictResolution::Exact(pkg.version(), pkg.id()),
            )?;

            // transitive closure of entry functions are fixed
            for dep_id in self.all_packages[object_id]
                .linkage_table()
                .values()
                .map(|info| &info.upgraded_id)
            {
                Self::add_and_unify(&mut unification_table, &self.all_packages, dep_id, |pkg| {
                    ConflictResolution::Exact(pkg.version(), pkg.id())
                })?;
            }
        }

        // Types can be fixed or not depending on config.
        for object_id in self.types.iter() {
            Self::add_and_unify(
                &mut unification_table,
                &self.all_packages,
                object_id,
                |pkg| {
                    if linking_config.fix_types {
                        ConflictResolution::Exact(pkg.version(), pkg.id())
                    } else {
                        ConflictResolution::AtLeast(pkg.version(), *object_id)
                    }
                },
            )?;
        }

        // Top level functions can be fixed or not depending on config. But they won't ever
        // transitively fix their dependencies.
        for object_id in self.non_entry_functions.iter() {
            Self::add_and_unify(
                &mut unification_table,
                &self.all_packages,
                object_id,
                |pkg| {
                    if linking_config.fix_top_level_functions {
                        ConflictResolution::Exact(pkg.version(), pkg.id())
                    } else {
                        ConflictResolution::AtLeast(pkg.version(), *object_id)
                    }
                },
            )?;
        }

        Ok(unification_table
            .into_values()
            .flat_map(|unifier| match unifier {
                ConflictResolution::Exact(_, object_id) => Some(object_id),
                ConflictResolution::AtLeast(_, object_id) => Some(object_id),
                ConflictResolution::Never(_) => None,
            })
            .collect())
    }

    // Add a package to the unification table, unifying it with any existing package in the table.
    // Errors if the packages cannot be unified (e.g., if one is exact and the other is not).
    fn add_and_unify(
        unification_table: &mut BTreeMap<ObjectID, ConflictResolution>,
        loaded_top_level_packages: &BTreeMap<ObjectID, MovePackage>,
        object_id: &ObjectID,
        resolution_fn: impl Fn(&MovePackage) -> ConflictResolution,
    ) -> anyhow::Result<()> {
        let package = loaded_top_level_packages
            .get(object_id)
            .ok_or_else(|| anyhow::anyhow!("Object {} not found in any package", object_id))?;

        let resolution = resolution_fn(package);

        if unification_table.contains_key(&package.original_package_id()) {
            let existing_unifier = unification_table
                .get_mut(&package.original_package_id())
                .expect("Guaranteed to exist");
            existing_unifier.unify(&resolution)?;
        } else {
            unification_table.insert(package.original_package_id(), resolution);
        }

        Ok(())
    }
}
