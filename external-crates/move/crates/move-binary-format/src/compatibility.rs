// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::inclusion_mode::{InclusionCheckExecutionMode, InclusionCheckMode};
use crate::{
    compatibility_mode::{CompatibilityMode, ExecutionCompatibilityMode},
    errors::{PartialVMError, PartialVMResult},
    file_format::{Ability, AbilitySet, DatatypeTyParameter, Visibility},
    file_format_common::VERSION_5,
    normalized::Module,
};
use move_core_types::identifier::Identifier;
use move_core_types::vm_status::StatusCode;
// ***************************************************************************
// ******************* IMPORTANT NOTE ON COMPATIBILITY ***********************
// ***************************************************************************
//
// If `check_datatype_layout` is false, type safety over a series of upgrades cannot be guaranteed
// for either structs or enums.
// This is because the type could first be removed, and then re-introduced with a diferent layout and/or
// additional variants in a later upgrade. E.g.,
// * For enums you could add a new variant even if `disallow_new_variants` is true, by first
//   removing the enum in an upgrade, and then reintroducing it with a new variant in a later
//   upgrade.
// * For structs you could remove a field from a struct and/or add another field by first removing
//   removing the struct in an upgrade and then reintroducing it with a different layout in a
//   later upgrade.

/// The result of a linking and layout compatibility check.
///
/// Here is what the different combinations of the compatibility flags mean:
/// `{ check_datatype_layout: true, check_private_entry_linking: true }`: fully backward compatible
/// `{ check_datatype_layout: true, check_private_entry_linking: false }`: Backwards compatible, private entry function signatures can change
/// `{ check_datatype_layout: true, check_private_entry_linking: true }`: Backward compatible, exclude the friend module declare and friend functions
/// `{ check_datatype_layout: true, check_private_entry_linking: false }`: Backward compatible, exclude the friend module declarations, friend functions, and private and friend entry function
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub struct Compatibility {
    /// if false, do not ensure the struct layout capability
    pub check_datatype_layout: bool,
    /// if false, treat `entry` as `private`
    pub check_private_entry_linking: bool,
    /// The set of abilities that cannot be added to an already exisiting type.
    pub disallowed_new_abilities: AbilitySet,
}

impl Default for Compatibility {
    fn default() -> Self {
        Self {
            check_datatype_layout: true,
            check_private_entry_linking: true,
            disallowed_new_abilities: AbilitySet::EMPTY,
        }
    }
}

impl Compatibility {
    pub fn full_check() -> Self {
        Self::default()
    }

    pub fn no_check() -> Self {
        Self {
            check_datatype_layout: false,
            check_private_entry_linking: false,
            disallowed_new_abilities: AbilitySet::EMPTY,
        }
    }

    /// Check compatibility for userspace module upgrades
    pub fn upgrade_check() -> Self {
        Self {
            check_datatype_layout: true,
            check_private_entry_linking: false,
            disallowed_new_abilities: AbilitySet::ALL,
        }
    }

    /// Check compatibility for system module upgrades
    pub fn framework_upgrade_check() -> Self {
        Self {
            check_datatype_layout: true,
            // Checking `entry` linkage is required because system packages are updated in-place, and a
            // transaction that was rolled back to make way for reconfiguration should still be runnable
            // after a reconfiguration that upgraded the framework.
            //
            // A transaction that calls a system function that was previously `entry` and is now private
            // will fail because its entrypoint became no longer callable. A transaction that calls a
            // system function that was previously `public entry` and is now just `public` could also
            // fail if one of its mutable inputs was being used in another private `entry` function.
            check_private_entry_linking: true,
            disallowed_new_abilities: AbilitySet::singleton(Ability::Key),
        }
    }

    pub fn need_check_compat(&self) -> bool {
        self != &Self::no_check()
    }

    /// Check compatibility for `new_module` relative to old module `old_module`.
    pub fn check(&self, old_module: &Module, new_module: &Module) -> PartialVMResult<()> {
        self.check_with_mode::<ExecutionCompatibilityMode>(old_module, new_module)
            .map_err(|_| PartialVMError::new(StatusCode::BACKWARD_INCOMPATIBLE_MODULE_UPDATE))
    }

    pub fn check_with_mode<M: CompatibilityMode>(
        &self,
        old_module: &Module,
        new_module: &Module,
    ) -> Result<(), M::Error> {
        let mut context = M::default();

        // module's name and address are unchanged
        if old_module.address != new_module.address || old_module.name != new_module.name {
            context.module_id_mismatch(
                &old_module.address,
                &old_module.name,
                &new_module.address,
                &new_module.name,
            );
        }

        // old module's structs are a subset of the new module's structs
        for (name, old_struct) in &old_module.structs {
            let Some(new_struct) = new_module.structs.get(name) else {
                // Struct not present in new . Existing modules that depend on this struct will fail to link with the new version of the module.
                // Also, struct layout cannot be guaranteed transitively, because after
                // removing the struct, it could be re-added later with a different layout.
                context.struct_missing(name, old_struct);
                continue;
            };

            if !datatype_abilities_compatible(
                self.disallowed_new_abilities,
                old_struct.abilities,
                new_struct.abilities,
            ) {
                context.struct_ability_mismatch(name, old_struct, new_struct);
            }

            if !datatype_type_parameters_compatible(
                self.check_datatype_layout,
                &old_struct.type_parameters,
                &new_struct.type_parameters,
            ) {
                context.struct_type_param_mismatch(name, old_struct, new_struct);
            }
            if new_struct.fields != old_struct.fields {
                // Fields changed. Code in this module will fail at runtime if it tries to
                // read a previously published struct value
                // TODO: this is a stricter definition than required. We could in principle
                // choose that changing the name (but not position or type) of a field is
                // compatible. The VM does not care about the name of a field
                // (it's purely informational), but clients presumably do.

                context.struct_field_mismatch(name, old_struct, new_struct);
            }
        }

        for (name, old_enum) in &old_module.enums {
            let Some(new_enum) = new_module.enums.get(name) else {
                // Enum not present in new. Existing modules that depend on this enum will fail to link with the new version of the module.
                // Also, enum layout cannot be guaranteed transitively, because after
                // removing the enum, it could be re-added later with a different layout.

                context.enum_missing(name, old_enum);
                continue;
            };

            if !datatype_abilities_compatible(
                self.disallowed_new_abilities,
                old_enum.abilities,
                new_enum.abilities,
            ) {
                context.enum_ability_mismatch(name, old_enum, new_enum);
            }

            if !datatype_type_parameters_compatible(
                self.check_datatype_layout,
                &old_enum.type_parameters,
                &new_enum.type_parameters,
            ) {
                context.enum_type_param_mismatch(name, old_enum, new_enum);
            }

            if new_enum.variants.len() > old_enum.variants.len() {
                context.enum_new_variant(name, old_enum, new_enum);
            }

            for (tag, old_variant) in old_enum.variants.iter().enumerate() {
                // If the new enum has fewer variants than the old one, datatype_layout is false
                // and we don't need to check the rest of the variants.
                let Some(new_variant) = new_enum.variants.get(tag) else {
                    context.enum_variant_missing(name, old_enum, tag);
                    continue;
                };
                if new_variant.name != old_variant.name {
                    // TODO: Variant renamed. This is a stricter definition than required.
                    // We could in principle choose that changing the name (but not position or
                    // type) of a variant is compatible. The VM does not care about the name of a
                    // variant if it's non-public (it's purely informational), but clients
                    // presumably would.
                    context.enum_variant_mismatch(name, old_enum, new_enum, tag);
                }
                if new_variant.fields != old_variant.fields {
                    // Fields changed. Code in this module will fail at runtime if it tries to
                    // read a previously published enum value
                    // TODO: this is a stricter definition than required. We could in principle
                    // choose that changing the name (but not position or type) of a field is
                    // compatible. The VM does not care about the name of a field
                    // (it's purely informational), but clients presumably do.
                    context.enum_variant_mismatch(name, old_enum, new_enum, tag);
                }
            }
        }

        // The modules are considered as compatible function-wise when all the conditions are met:
        //
        // - old module's public functions are a subset of the new module's public functions
        //   (i.e. we cannot remove or change public functions)
        // - old module's script functions are a subset of the new module's script functions
        //   (i.e. we cannot remove or change script functions)
        for (name, old_func) in &old_module.functions {
            // Check for removed public functions
            let Some(new_func) = new_module.functions.get(name) else {
                if old_func.visibility == Visibility::Public {
                    context.function_missing_public(name, old_func);
                } else if old_func.is_entry && self.check_private_entry_linking {
                    // This must be a private entry function. So set the link breakage if we're
                    // checking for that.
                    context.function_missing_entry(name, old_func);
                }
                continue;
            };

            // Check visibility compatibility
            if old_func.visibility == Visibility::Public
                && new_func.visibility != Visibility::Public
            {
                context.function_lost_public_visibility(name, old_func);
            }

            // Check entry compatibility
            #[allow(clippy::if_same_then_else)]
            if old_module.file_format_version < VERSION_5
                && new_module.file_format_version < VERSION_5
                && old_func.visibility != Visibility::Private
                && old_func.is_entry != new_func.is_entry
            {
                context.function_entry_compatibility(name, old_func, new_func);
            } else if old_func.is_entry && !new_func.is_entry {
                context.function_entry_compatibility(name, old_func, new_func);
            }

            // Check signature compatibility
            if old_func.parameters != new_func.parameters
                || old_func.return_ != new_func.return_
                || !fun_type_parameters_compatible(
                    &old_func.type_parameters,
                    &new_func.type_parameters,
                )
            {
                context.function_signature_mismatch(name, old_func, new_func);
            }
        }

        context.finish(self)
    }
}

// When upgrading, the new abilities must be a superset of the old abilities.
// Adding an ability is fine as long as it's not in the disallowed_new_abilities,
// but removing an ability could cause existing usages to fail.
fn datatype_abilities_compatible(
    disallowed_new_abilities: AbilitySet,
    old_abilities: AbilitySet,
    new_abilities: AbilitySet,
) -> bool {
    old_abilities.is_subset(new_abilities)
        && disallowed_new_abilities.into_iter().all(|ability| {
            // If the new abilities have the ability the old ones must have it to
            !new_abilities.has_ability(ability) || old_abilities.has_ability(ability)
        })
}

// When upgrading, the new type parameters must be the same length, and the new type parameter
// constraints must be compatible
fn fun_type_parameters_compatible(
    old_type_parameters: &[AbilitySet],
    new_type_parameters: &[AbilitySet],
) -> bool {
    old_type_parameters.len() == new_type_parameters.len()
        && old_type_parameters.iter().zip(new_type_parameters).all(
            |(old_type_parameter_constraint, new_type_parameter_constraint)| {
                type_parameter_constraints_compatible(
                    false, // generic abilities can change for functions
                    *old_type_parameter_constraint,
                    *new_type_parameter_constraint,
                )
            },
        )
}

fn datatype_type_parameters_compatible(
    disallow_changing_generic_abilities: bool,
    old_type_parameters: &[DatatypeTyParameter],
    new_type_parameters: &[DatatypeTyParameter],
) -> bool {
    old_type_parameters.len() == new_type_parameters.len()
        && old_type_parameters.iter().zip(new_type_parameters).all(
            |(old_type_parameter, new_type_parameter)| {
                type_parameter_phantom_decl_compatible(
                    disallow_changing_generic_abilities,
                    old_type_parameter,
                    new_type_parameter,
                ) && type_parameter_constraints_compatible(
                    disallow_changing_generic_abilities,
                    old_type_parameter.constraints,
                    new_type_parameter.constraints,
                )
            },
        )
}

// When upgrading, the new constraints must be a subset of (or equal to) the old constraints.
// Removing an ability is fine, but adding an ability could cause existing callsites to fail
fn type_parameter_constraints_compatible(
    disallow_changing_generic_abilities: bool,
    old_type_constraints: AbilitySet,
    new_type_constraints: AbilitySet,
) -> bool {
    if disallow_changing_generic_abilities {
        old_type_constraints == new_type_constraints
    } else {
        new_type_constraints.is_subset(old_type_constraints)
    }
}

// Adding a phantom annotation to a parameter won't break clients because that can only increase the
// the set of abilities in struct instantiations. Put it differently, adding phantom declarations
// relaxes the requirements for clients.
fn type_parameter_phantom_decl_compatible(
    disallow_changing_generic_abilities: bool,
    old_type_parameter: &DatatypeTyParameter,
    new_type_parameter: &DatatypeTyParameter,
) -> bool {
    if disallow_changing_generic_abilities {
        // phantom/non-phantom cannot change from one version to the next.
        old_type_parameter.is_phantom == new_type_parameter.is_phantom
    } else {
        // old_type_paramter.is_phantom => new_type_parameter.is_phantom
        !old_type_parameter.is_phantom || new_type_parameter.is_phantom
    }
}

/// A simpler, and stricter compatibility checker relating to the inclusion of the old module in
/// the new.
#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum InclusionCheck {
    Subset,
    Equal,
}

impl InclusionCheck {
    pub fn check(&self, old_module: &Module, new_module: &Module) -> PartialVMResult<()> {
        self.check_with_mode::<InclusionCheckExecutionMode>(old_module, new_module)
            .map_err(|_| PartialVMError::new(StatusCode::BACKWARD_INCOMPATIBLE_MODULE_UPDATE))
    }

    // Check that all code in `old_module` is included `new_module`. If `Exact` no new code can be
    // in `new_module` (Note: `new_module` may have larger pools, but they are not accessed by the
    // code).
    pub fn check_with_mode<M: InclusionCheckMode>(
        &self,
        old_module: &Module,
        new_module: &Module,
    ) -> Result<(), M::Error> {
        let mut context = M::default();

        // Module checks
        if old_module.name != new_module.name {
            context.module_name_mismatch(&old_module.name, &new_module.name);
        }

        if old_module.address != new_module.address {
            context.module_address_mismatch(&old_module.address, &new_module.address);
        }

        if old_module.file_format_version > new_module.file_format_version {
            context.file_format_version_downgrade(
                old_module.file_format_version,
                new_module.file_format_version,
            );
        }

        // Since the structs are sorted we can iterate through each list to find the differences using two pointers
        for mark in compare_identifiers(old_module.structs.keys(), new_module.structs.keys()) {
            match mark {
                Mark::New(name) => context.struct_new(name, new_module.structs.get(name).unwrap()),
                Mark::Missing(name) => context.struct_missing(name),
                Mark::Existing(name) => {
                    let old_struct = old_module.structs.get(name).unwrap();
                    let new_struct = new_module.structs.get(name).unwrap();
                    if old_struct != new_struct {
                        context.struct_change(name, old_struct, new_struct);
                    }
                }
            }
        }
        // enum checks
        compare_identifiers(old_module.enums.keys(), new_module.enums.keys()).for_each(|mark| {
            match mark {
                Mark::New(name) => context.enum_new(name, new_module.enums.get(name).unwrap()),
                Mark::Missing(name) => context.enum_missing(name),
                Mark::Existing(name) => {
                    let old_enum = old_module.enums.get(name).unwrap();
                    let new_enum = new_module.enums.get(name).unwrap();
                    if old_enum != new_enum {
                        context.enum_change(name, old_enum);
                    }
                }
            }
        });

        // function checks

        compare_identifiers(old_module.functions.keys(), new_module.functions.keys()).for_each(
            |mark| match mark {
                Mark::New(name) => {
                    context.function_new(name, new_module.functions.get(name).unwrap())
                }
                Mark::Missing(name) => context.function_missing(name),
                Mark::Existing(name) => {
                    let old_function = old_module.functions.get(name).unwrap();
                    let new_function = new_module.functions.get(name).unwrap();
                    if old_function != new_function {
                        context.function_change(name, old_function);
                    }
                }
            },
        );

        // use iters for friends
        let mut old_friend_iter = old_module.friends.iter().peekable();
        let mut new_friend_iter = new_module.friends.iter().peekable();
        loop {
            let old_friend = old_friend_iter.peek();
            let new_friend = new_friend_iter.peek();
            match (old_friend, new_friend) {
                (Some(old_friend), Some(new_friend)) => {
                    if old_friend != new_friend {
                        if old_friend.lt(new_friend) {
                            context.friend_missing(old_friend);
                            old_friend_iter.next();
                        } else {
                            context.friend_new(new_friend);
                            new_friend_iter.next();
                        }
                    } else {
                        old_friend_iter.next();
                        new_friend_iter.next();
                    }
                }
                (Some(old_friend), None) => {
                    context.friend_missing(old_friend);
                    old_friend_iter.next();
                }
                (None, Some(new_friend)) => {
                    context.friend_new(new_friend);
                    new_friend_iter.next();
                }
                (None, None) => {
                    break;
                }
            }
        }

        context.finish(self)
    }
}

enum Mark<'a> {
    New(&'a Identifier),
    Missing(&'a Identifier),
    Existing(&'a Identifier),
}

fn compare_identifiers<'c, I, J>(old: I, new: J) -> impl Iterator<Item = Mark<'c>> + 'c
where
    I: Iterator<Item = &'c Identifier> + 'c,
    J: Iterator<Item = &'c Identifier> + 'c,
{
    let mut old = old.peekable();
    let mut new = new.peekable();
    std::iter::from_fn(move || match (old.peek(), new.peek()) {
        (Some(old_name), Some(new_name)) => match old_name.cmp(new_name) {
            std::cmp::Ordering::Equal => {
                let old_name = *old_name;
                old.next();
                new.next();
                Some(Mark::Existing(old_name))
            }
            std::cmp::Ordering::Less => {
                let old_name = *old_name;
                old.next();
                Some(Mark::Missing(old_name))
            }
            std::cmp::Ordering::Greater => {
                let new_name = *new_name;
                new.next();
                Some(Mark::New(new_name))
            }
        },
        (Some(old_name), None) => {
            let name = *old_name;
            old.next();
            Some(Mark::Missing(name))
        }
        (None, Some(new_name)) => {
            let name = *new_name;
            new.next();
            Some(Mark::New(name))
        }
        (None, None) => None,
    })
}
