// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use crate::{
    errors::{PartialVMError, PartialVMResult},
    file_format::{AbilitySet, DatatypeTyParameter, Visibility},
    file_format_common::VERSION_5,
    normalized::Module,
};
use move_core_types::vm_status::StatusCode;

// ***************************************************************************
// ******************* IMPORTANT NOTE ON COMPATIBILITY ***********************
// ***************************************************************************
//
// If `check_datatype_layout` and/or `check_datatype_and_pub_function_linking` is false, type
// safety over a series of upgrades cannot be guaranteed for either structs or enums. This is
// because the type could first be removed, and then re-introduced with a diferent layout and/or
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
/// `{ check_datatype_and_pub_function_linking: true, check_datatype_layout: true, check_friend_linking: true, check_private_entry_linking: true }`: fully backward compatible
/// `{ check_datatype_and_pub_function_linking: true, check_datatype_layout: true, check_friend_linking: true, check_private_entry_linking: false }`: Backwards compatible, private entry function signatures can change
/// `{ check_datatype_and_pub_function_linking: true, check_datatype_layout: true, check_friend_linking: false, check_private_entry_linking: true }`: Backward compatible, exclude the friend module declare and friend functions
/// `{ check_datatype_and_pub_function_linking: true, check_datatype_layout: true, check_friend_linking: false, check_private_entry_linking: false }`: Backward compatible, exclude the friend module declarations, friend functions, and private and friend entry function
/// `{ check_datatype_and_pub_function_linking: false, check_datatype_layout: true, check_friend_linking: false, check_private_entry_linking: _ }`: Dependent modules that reference functions or types in this module may not link. However, fixing, recompiling, and redeploying all dependent modules will work--no data migration needed.
/// `{ check_datatype_and_pub_function_linking: true, check_datatype_layout: false, check_friend_linking: true, check_private_entry_linking: _ }`: Attempting to read structs published by this module will now fail at runtime. However, dependent modules will continue to link. Requires data migration, but no changes to dependent modules.
/// `{ check_datatype_and_pub_function_linking: false, check_datatype_layout: false, check_friend_linking: false, check_private_entry_linking: _ }`: Everything is broken. Need both a data migration and changes to dependent modules.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub struct Compatibility {
    /// if false, do not ensure the dependent modules that reference public functions or structs in this module can link
    pub check_datatype_and_pub_function_linking: bool,
    /// if false, do not ensure the struct layout capability
    pub check_datatype_layout: bool,
    /// if false, treat `friend` as `private` when `check_datatype_and_pub_function_linking`.
    pub check_friend_linking: bool,
    /// if false, treat `entry` as `private` when `check_datatype_and_pub_function_linking`.
    pub check_private_entry_linking: bool,
    /// The set of abilities that cannot be added to an already exisiting type.
    pub disallowed_new_abilities: AbilitySet,
    /// Don't allow generic type parameters in structs to change their abilities or constraints.
    pub disallow_change_datatype_type_params: bool,
    /// Don't allow adding new variants at the end of an enum.
    pub disallow_new_variants: bool,
}

impl Default for Compatibility {
    fn default() -> Self {
        Self {
            check_datatype_and_pub_function_linking: true,
            check_datatype_layout: true,
            check_friend_linking: true,
            check_private_entry_linking: true,
            disallowed_new_abilities: AbilitySet::EMPTY,
            disallow_change_datatype_type_params: true,
            disallow_new_variants: true,
        }
    }
}

impl Compatibility {
    pub fn full_check() -> Self {
        Self::default()
    }

    pub fn no_check() -> Self {
        Self {
            check_datatype_and_pub_function_linking: false,
            check_datatype_layout: false,
            check_friend_linking: false,
            check_private_entry_linking: false,
            disallowed_new_abilities: AbilitySet::EMPTY,
            disallow_change_datatype_type_params: false,
            disallow_new_variants: false,
        }
    }

    pub fn need_check_compat(&self) -> bool {
        self != &Self::no_check()
    }

    /// Check compatibility for `new_module` relative to old module `old_module`.
    pub fn check(&self, old_module: &Module, new_module: &Module) -> PartialVMResult<()> {
        let mut datatype_and_function_linking = true;
        let mut datatype_layout = true;
        let mut friend_linking = true;
        let mut entry_linking = true;
        let mut no_new_variants = true;

        // module's name and address are unchanged
        if old_module.address != new_module.address || old_module.name != new_module.name {
            datatype_and_function_linking = false;
        }

        // old module's structs are a subset of the new module's structs
        for (name, old_struct) in &old_module.structs {
            let Some(new_struct) = new_module.structs.get(name) else {
                // Struct not present in new . Existing modules that depend on this struct will fail to link with the new version of the module.
                // Also, struct layout cannot be guaranteed transitively, because after
                // removing the struct, it could be re-added later with a different layout.
                datatype_and_function_linking = false;
                datatype_layout = false;
                break;
            };

            if !datatype_abilities_compatible(
                self.disallowed_new_abilities,
                old_struct.abilities,
                new_struct.abilities,
            ) || !datatype_type_parameters_compatible(
                self.disallow_change_datatype_type_params,
                &old_struct.type_parameters,
                &new_struct.type_parameters,
            ) {
                datatype_and_function_linking = false;
            }
            if new_struct.fields != old_struct.fields {
                // Fields changed. Code in this module will fail at runtime if it tries to
                // read a previously published struct value
                // TODO: this is a stricter definition than required. We could in principle
                // choose that changing the name (but not position or type) of a field is
                // compatible. The VM does not care about the name of a field
                // (it's purely informational), but clients presumably do.
                datatype_layout = false
            }
        }

        for (name, old_enum) in &old_module.enums {
            let Some(new_enum) = new_module.enums.get(name) else {
                // Enum not present in new. Existing modules that depend on this enum will fail to link with the new version of the module.
                // Also, enum layout cannot be guaranteed transitively, because after
                // removing the enum, it could be re-added later with a different layout.
                datatype_and_function_linking = false;
                datatype_layout = false;
                break;
            };

            if !datatype_abilities_compatible(
                self.disallowed_new_abilities,
                old_enum.abilities,
                new_enum.abilities,
            ) || !datatype_type_parameters_compatible(
                self.disallow_change_datatype_type_params,
                &old_enum.type_parameters,
                &new_enum.type_parameters,
            ) {
                datatype_and_function_linking = false;
            }

            if new_enum.variants.len() > old_enum.variants.len() {
                no_new_variants = false;
            }

            if new_enum.variants.len() < old_enum.variants.len() {
                datatype_layout = false;
            }

            for (tag, old_variant) in old_enum.variants.iter().enumerate() {
                // If the new enum has fewer variants than the old one, datatype_layout is false
                // and we don't need to check the rest of the variants.
                let Some(new_variant) = new_enum.variants.get(tag) else {
                    datatype_layout = false;
                    break;
                };
                if new_variant.name != old_variant.name {
                    // TODO: Variant renamed. This is a stricter definition than required.
                    // We could in principle choose that changing the name (but not position or
                    // type) of a variant is compatible. The VM does not care about the name of a
                    // variant if it's non-public (it's purely informational), but clients
                    // presumably would.
                    datatype_layout = false;
                }
                if new_variant.fields != old_variant.fields {
                    // Fields changed. Code in this module will fail at runtime if it tries to
                    // read a previously published enum value
                    // TODO: this is a stricter definition than required. We could in principle
                    // choose that changing the name (but not position or type) of a field is
                    // compatible. The VM does not care about the name of a field
                    // (it's purely informational), but clients presumably do.
                    datatype_layout = false
                }
            }
        }

        // The modules are considered as compatible function-wise when all the conditions are met:
        //
        // - old module's public functions are a subset of the new module's public functions
        //   (i.e. we cannot remove or change public functions)
        // - old module's script functions are a subset of the new module's script functions
        //   (i.e. we cannot remove or change script functions)
        // - for any friend function that is removed or changed in the old module
        //   - if the function visibility is upgraded to public, it is OK
        //   - otherwise, it is considered as incompatible.
        //
        // NOTE: it is possible to relax the compatibility checking for a friend function, i.e.,
        // we can remove/change a friend function if the function is not used by any module in the
        // friend list. But for simplicity, we decided to go to the more restrictive form now and
        // we may revisit this in the future.
        for (name, old_func) in &old_module.functions {
            let Some(new_func) = new_module.functions.get(name) else {
                if old_func.visibility == Visibility::Friend {
                    friend_linking = false;
                } else if old_func.visibility != Visibility::Private {
                    datatype_and_function_linking = false;
                } else if old_func.is_entry && self.check_private_entry_linking {
                    // This must be a private entry function. So set the link breakage if we're
                    // checking for that.
                    entry_linking = false;
                }
                continue;
            };

            // Check visibility compatibility
            match (old_func.visibility, new_func.visibility) {
                (Visibility::Public, Visibility::Private | Visibility::Friend) => {
                    datatype_and_function_linking = false
                }
                (Visibility::Friend, Visibility::Private) => friend_linking = false,
                _ => (),
            }

            // Check entry compatibility
            if old_module.file_format_version < VERSION_5
                && new_module.file_format_version < VERSION_5
                && old_func.visibility != Visibility::Private
                && old_func.is_entry != new_func.is_entry
            {
                entry_linking = false
            } else if old_func.is_entry && !new_func.is_entry {
                entry_linking = false;
            }

            // Check signature compatibility
            if old_func.parameters != new_func.parameters
                || old_func.return_ != new_func.return_
                || !fun_type_parameters_compatible(
                    &old_func.type_parameters,
                    &new_func.type_parameters,
                )
            {
                match old_func.visibility {
                    Visibility::Friend => friend_linking = false,
                    Visibility::Public => datatype_and_function_linking = false,
                    Visibility::Private => (),
                }

                if old_func.is_entry {
                    entry_linking = false;
                }
            }
        }

        // check friend declarations compatibility
        //
        // - additions to the list are allowed
        // - removals are not allowed
        //
        let old_friend_module_ids: BTreeSet<_> = old_module.friends.iter().cloned().collect();
        let new_friend_module_ids: BTreeSet<_> = new_module.friends.iter().cloned().collect();
        if !old_friend_module_ids.is_subset(&new_friend_module_ids) {
            friend_linking = false;
        }

        if self.check_datatype_and_pub_function_linking && !datatype_and_function_linking {
            return Err(PartialVMError::new(
                StatusCode::BACKWARD_INCOMPATIBLE_MODULE_UPDATE,
            ));
        }
        if self.check_datatype_layout && !datatype_layout {
            return Err(PartialVMError::new(
                StatusCode::BACKWARD_INCOMPATIBLE_MODULE_UPDATE,
            ));
        }
        if self.check_friend_linking && !friend_linking {
            return Err(PartialVMError::new(
                StatusCode::BACKWARD_INCOMPATIBLE_MODULE_UPDATE,
            ));
        }
        if self.check_private_entry_linking && !entry_linking {
            return Err(PartialVMError::new(
                StatusCode::BACKWARD_INCOMPATIBLE_MODULE_UPDATE,
            ));
        }
        if self.disallow_new_variants && !no_new_variants {
            return Err(PartialVMError::new(
                StatusCode::BACKWARD_INCOMPATIBLE_MODULE_UPDATE,
            ));
        }
        Ok(())
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
    // Check that all code in `old_module` is included `new_module`. If `Exact` no new code can be
    // in `new_module` (Note: `new_module` may have larger pools, but they are not accessed by the
    // code).
    pub fn check(&self, old_module: &Module, new_module: &Module) -> PartialVMResult<()> {
        let err = Err(PartialVMError::new(
            StatusCode::BACKWARD_INCOMPATIBLE_MODULE_UPDATE,
        ));

        // Module checks
        if old_module.address != new_module.address
            || old_module.name != new_module.name
            || old_module.file_format_version > new_module.file_format_version
        {
            return err;
        }

        // If we're checking exactness we make sure there's an inclusion, and that the size of all
        // of the tables are the exact same except for constants.
        if (self == &Self::Equal)
            && (old_module.structs.len() != new_module.structs.len()
                || old_module.enums.len() != new_module.enums.len()
                || old_module.functions.len() != new_module.functions.len()
                || old_module.friends.len() != new_module.friends.len())
        {
            return err;
        }

        // Struct checks
        for (name, old_struct) in &old_module.structs {
            match new_module.structs.get(name) {
                Some(new_struct) if old_struct == new_struct => (),
                _ => {
                    return err;
                }
            };
        }

        // Enum checks
        for (name, old_enum) in &old_module.enums {
            let Some(new_enum) = new_module.enums.get(name) else {
                return err;
            };

            if old_enum.abilities != new_enum.abilities {
                return err;
            }
            if old_enum.type_parameters != new_enum.type_parameters {
                return err;
            }
            if old_enum.variants.len() > new_enum.variants.len() {
                return err;
            }

            // NB: In the future if we allow adding new variants to enums in subset mode
            // remove this if statement. This check is somewhat redundant with the one
            // below, the one below should be kept if we allow adding variants in subset
            // mode.
            if old_enum.variants.len() != new_enum.variants.len() {
                return err;
            }

            if self == &Self::Equal && old_enum.variants.len() != new_enum.variants.len() {
                return err;
            }
            // NB: We are using the fact that the variants are sorted by tag, that we've
            // already ensured that the old variants are >= new variants, and the fact
            // that zip will truncate the second iterator if there are extra there to allow
            // adding new variants to enums in `Self::Subset` compatibility mode.
            if !old_enum
                .variants
                .iter()
                .zip(&new_enum.variants)
                .all(|(old, new)| old == new)
            {
                return err;
            }
        }

        // Function checks
        for (name, old_func) in &old_module.functions {
            match new_module
                .functions
                .get(name)
                .or_else(|| new_module.functions.get(name))
            {
                Some(new_func) if old_func == new_func => (),
                _ => {
                    return err;
                }
            }
        }

        Ok(())
    }
}
