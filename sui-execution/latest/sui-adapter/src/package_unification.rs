use move_binary_format::{binary_config::BinaryConfig, file_format::Visibility};
use std::collections::BTreeMap;
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    move_package::MovePackage,
    storage::BackingPackageStore,
    transaction::{Command, ProgrammableTransaction},
    type_input::TypeInput,
};

#[derive(Debug)]
pub struct PTBLinkageMetadata {
    pub config: LinkageConfig,
    pub unification_table: BTreeMap<ObjectID, ConflictResolution>,
    pub all_packages: BTreeMap<ObjectID, MovePackage>,
}

#[derive(Debug)]
pub struct LinkageConfig {
    pub fix_top_level_functions: bool,
    pub fix_types: bool,
    pub exact_entry_transitive_deps: bool,
}

#[derive(Debug)]
pub enum ConflictResolution {
    Exact(SequenceNumber, ObjectID),
    AtLeast(SequenceNumber, ObjectID),
}

impl LinkageConfig {
    pub fn loose() -> Self {
        Self {
            fix_top_level_functions: true,
            fix_types: false,
            exact_entry_transitive_deps: false,
        }
    }

    pub fn generate_top_level_fn_constraint(
        &self,
    ) -> for<'a> fn(&'a MovePackage) -> ConflictResolution {
        if self.fix_top_level_functions {
            ConflictResolution::exact
        } else {
            ConflictResolution::at_least
        }
    }

    pub fn generate_type_constraint(&self) -> for<'a> fn(&'a MovePackage) -> ConflictResolution {
        if self.fix_types {
            ConflictResolution::exact
        } else {
            ConflictResolution::at_least
        }
    }

    pub fn generate_entry_transitive_dep_constraint(
        &self,
    ) -> for<'a> fn(&'a MovePackage) -> ConflictResolution {
        if self.exact_entry_transitive_deps {
            ConflictResolution::exact
        } else {
            ConflictResolution::at_least
        }
    }
}

impl ConflictResolution {
    pub fn exact<'a>(pkg: &MovePackage) -> ConflictResolution {
        ConflictResolution::Exact(pkg.version(), pkg.id())
    }

    pub fn at_least<'a>(pkg: &MovePackage) -> ConflictResolution {
        ConflictResolution::AtLeast(pkg.version(), pkg.id())
    }

    pub fn unify(&self, other: &ConflictResolution) -> anyhow::Result<ConflictResolution> {
        match (&self, other) {
            // If we have two exact resolutions, they must be the same.
            (ConflictResolution::Exact(sv, self_id), ConflictResolution::Exact(ov, other_id)) => {
                if self_id != other_id || sv != ov {
                    return Err(anyhow::anyhow!(
                        "UNIFICATION ERROR: Exact/exact conflicting resolutions for packages: {self_id}@{sv} and {other_id}@{ov}",
                    ));
                } else {
                    Ok(ConflictResolution::Exact(*sv, *self_id))
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

                Ok(ConflictResolution::AtLeast(
                    *self_version.max(other_version),
                    id,
                ))
            }
            // If you unify an exact and an at least, the exact must be greater than or equal to
            // the at least. It unifies to an exact.
            (
                ConflictResolution::Exact(exact_version, exact_id),
                ConflictResolution::AtLeast(at_least_version, at_least_id),
            )
            | (
                ConflictResolution::AtLeast(at_least_version, at_least_id),
                ConflictResolution::Exact(exact_version, exact_id),
            ) => {
                if exact_version < at_least_version {
                    return Err(anyhow::anyhow!(
                        "UNIFICATION ERROR: Exact/at least Conflicting resolutions for packages: Exact {exact_id}@{exact_version} and {at_least_id}@{at_least_version}",
                    ));
                }

                Ok(ConflictResolution::Exact(*exact_version, *exact_id))
            }
        }
    }
}

impl PTBLinkageMetadata {
    pub fn from_ptb(
        ptb: &ProgrammableTransaction,
        store: &dyn BackingPackageStore,
        config: LinkageConfig,
        binary_config: &BinaryConfig,
    ) -> anyhow::Result<Self> {
        let mut linkage = PTBLinkageMetadata {
            unification_table: BTreeMap::new(),
            all_packages: BTreeMap::new(),
            config,
        };

        for command in &ptb.commands {
            linkage.add_command(command, store, binary_config)?;
        }

        Ok(linkage)
    }

    fn add_command(
        &mut self,
        command: &Command,
        store: &dyn BackingPackageStore,
        binary_config: &BinaryConfig,
    ) -> anyhow::Result<()> {
        match command {
            Command::MoveCall(programmable_move_call) => {
                let pkg = self.get_package(&programmable_move_call.package, store)?;

                let m = pkg
                    .deserialize_module_by_name(&programmable_move_call.module, binary_config)
                    .map_err(|e| anyhow::anyhow!("Error deserializing module: {:?}", e))?;
                let Some(fdef) = m.function_defs().into_iter().find(|f| {
                    m.identifier_at(m.function_handle_at(f.function).name)
                        .as_str()
                        == &programmable_move_call.function
                }) else {
                    return Err(anyhow::anyhow!(
                        "Function {} not found in module {}",
                        programmable_move_call.function,
                        programmable_move_call.module
                    ));
                };

                let pkg_id = pkg.id();
                let transitive_deps = pkg
                    .linkage_table()
                    .values()
                    .map(|info| info.upgraded_id)
                    .collect::<Vec<_>>();

                for ty in &programmable_move_call.type_arguments {
                    self.add_type(ty, store)?;
                }

                // Register function entrypoint
                if fdef.is_entry && fdef.visibility != Visibility::Public {
                    self.add_and_unify(&pkg_id, store, ConflictResolution::exact)?;

                    // transitive closure of entry functions are fixed
                    for object_id in transitive_deps.iter() {
                        self.add_and_unify(
                            object_id,
                            store,
                            self.config.generate_entry_transitive_dep_constraint(),
                        )?;
                    }
                } else {
                    self.add_and_unify(
                        &pkg_id,
                        store,
                        self.config.generate_top_level_fn_constraint(),
                    )?;

                    // transitive closure of non-entry functions are at-least
                    for object_id in transitive_deps.iter() {
                        self.add_and_unify(object_id, store, ConflictResolution::at_least)?;
                    }
                }
            }
            Command::MakeMoveVec(type_input, _) => {
                if let Some(ty) = type_input {
                    self.add_type(ty, store)?;
                }
            }
            // Upgrades and Publishes don't count toward the global linkage determination.
            Command::Upgrade(_, _, _, _)
            | Command::Publish(_, _)
            | Command::TransferObjects(_, _)
            | Command::SplitCoins(_, _)
            | Command::MergeCoins(_, _) => (),
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
                    self.add_and_unify(
                        &ObjectID::from(struct_input.address),
                        store,
                        self.config.generate_type_constraint(),
                    )?;
                    for ty in struct_input.type_params.iter() {
                        stack.push(ty);
                    }
                }
            }
        }
        Ok(())
    }

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

    // Add a package to the unification table, unifying it with any existing package in the table.
    // Errors if the packages cannot be unified (e.g., if one is exact and the other is not).
    fn add_and_unify(
        &mut self,
        object_id: &ObjectID,
        store: &dyn BackingPackageStore,
        resolution_fn: fn(&MovePackage) -> ConflictResolution,
    ) -> anyhow::Result<()> {
        let package = self.get_package(object_id, store)?;

        let resolution = resolution_fn(package);
        let original_pkg_id = package.original_package_id();

        if self.unification_table.contains_key(&original_pkg_id) {
            let existing_unifier = self
                .unification_table
                .get_mut(&original_pkg_id)
                .expect("Guaranteed to exist");
            *existing_unifier = existing_unifier.unify(&resolution)?;
        } else {
            self.unification_table.insert(original_pkg_id, resolution);
        }

        Ok(())
    }
}
