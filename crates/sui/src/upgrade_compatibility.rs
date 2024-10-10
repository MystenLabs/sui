// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[path = "unit_tests/upgrade_compatibility_tests.rs"]
#[cfg(test)]
mod upgrade_compatibility_tests;

use std::collections::HashMap;
use std::fs;

use anyhow::{anyhow, Context, Error};
use codespan_reporting::diagnostic::{Diagnostic, Label};
use codespan_reporting::files::SimpleFiles;
use codespan_reporting::term::termcolor::{ColorChoice, StandardStream};

use move_binary_format::{
    compatibility::Compatibility,
    compatibility_mode::CompatibilityMode,
    file_format::Visibility,
    normalized::{Enum, Function, Module, Struct},
    CompiledModule,
};
use move_core_types::{
    account_address::AccountAddress,
    identifier::{IdentStr, Identifier},
};
use move_package::compilation::compiled_package::CompiledUnitWithSource;
use sui_json_rpc_types::{SuiObjectDataOptions, SuiRawData};
use sui_move_build::CompiledPackage;
use sui_protocol_config::ProtocolConfig;
use sui_sdk::SuiClient;
use sui_types::{base_types::ObjectID, execution_config_utils::to_binary_config};

/// Errors that can occur during upgrade compatibility checks.
/// one-to-one related to the underlying trait functions see: [`CompatibilityMode`]
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) enum UpgradeCompatibilityModeError {
    StructMissing {
        name: Identifier,
        old_struct: Struct,
    },
    StructAbilityMismatch {
        name: Identifier,
        old_struct: Struct,
        new_struct: Struct,
    },
    StructTypeParamMismatch {
        name: Identifier,
        old_struct: Struct,
        new_struct: Struct,
    },
    StructFieldMismatch {
        name: Identifier,
        old_struct: Struct,
        new_struct: Struct,
    },
    EnumMissing {
        name: Identifier,
        old_enum: Enum,
    },
    EnumAbilityMismatch {
        name: Identifier,
        old_enum: Enum,
        new_enum: Enum,
    },
    EnumTypeParamMismatch {
        name: Identifier,
        old_enum: Enum,
        new_enum: Enum,
    },
    EnumNewVariant {
        name: Identifier,
        old_enum: Enum,
        new_enum: Enum,
    },
    EnumVariantMissing {
        name: Identifier,
        old_enum: Enum,
        tag: usize,
    },
    EnumVariantMismatch {
        name: Identifier,
        old_enum: Enum,
        new_enum: Enum,
        tag: usize,
    },
    FunctionMissingPublic {
        name: Identifier,
        old_function: Function,
    },
    FunctionMissingEntry {
        name: Identifier,
        old_function: Function,
    },
    FunctionSignatureMismatch {
        name: Identifier,
        old_function: Function,
        new_function: Function,
    },
    FunctionLostPublicVisibility {
        name: Identifier,
        old_function: Function,
    },
    FunctionEntryCompatibility {
        name: Identifier,
        old_function: Function,
        new_function: Function,
    },
}

/// A list of errors that can occur during upgrade compatibility checks.
#[derive(Debug, Clone, Default)]
pub struct UpgradeErrorList {
    errors: Vec<UpgradeCompatibilityModeError>,
    source: Option<String>,
}

impl UpgradeErrorList {
    fn push(&mut self, err: UpgradeCompatibilityModeError) {
        self.errors.push(err);
    }

    /// Only keep the errors that break compatibility with the given [`Compatibility`]
    fn retain_incompatible(&mut self, compatibility: &Compatibility) {
        self.errors
            .retain(|e| e.breaks_compatibility(compatibility));
    }

    /// Print the errors to the console with the relevant source code.
    fn print_errors(
        &mut self,
        compiled_unit_with_source: &CompiledUnitWithSource,
    ) -> Result<(), Error> {
        for err in self.errors.clone() {
            match err {
                UpgradeCompatibilityModeError::StructMissing { name, .. } => {
                    self.print_missing_definition(
                        "Struct",
                        name.to_string(),
                        compiled_unit_with_source,
                    )?;
                }
                UpgradeCompatibilityModeError::EnumMissing { name, .. } => {
                    self.print_missing_definition(
                        "Enum",
                        name.to_string(),
                        compiled_unit_with_source,
                    )?;
                }
                UpgradeCompatibilityModeError::FunctionMissingPublic { name, .. } => {
                    self.print_missing_definition(
                        "Function",
                        name.to_string(),
                        compiled_unit_with_source,
                    )?;
                }
                UpgradeCompatibilityModeError::FunctionMissingEntry { name, .. } => {
                    self.print_missing_definition(
                        "Function",
                        name.to_string(),
                        compiled_unit_with_source,
                    )?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// retrieve the source, caches the source after the first read
    fn source(
        &mut self,
        compiled_unit_with_source: &CompiledUnitWithSource,
    ) -> Result<&String, Error> {
        if self.source.is_none() {
            let source_path = compiled_unit_with_source.source_path.clone();
            let source_content = fs::read_to_string(&source_path)?;
            self.source = Some(source_content);
        }
        Ok(self.source.as_ref().unwrap())
    }

    /// Print missing definition errors, e.g. struct, enum, function
    fn print_missing_definition(
        &mut self,
        declaration_kind: &str,
        identifier_name: String,
        compiled_unit_with_source: &CompiledUnitWithSource,
    ) -> Result<(), Error> {
        let module_name = compiled_unit_with_source.unit.name;
        let source_path = compiled_unit_with_source.source_path.to_string_lossy();
        let source = self.source(compiled_unit_with_source)?;

        let start = compiled_unit_with_source
            .unit
            .source_map
            .definition_location
            .start() as usize;

        let end = compiled_unit_with_source
            .unit
            .source_map
            .definition_location
            .end() as usize;

        let mut files = SimpleFiles::new();
        let file_id = files.add(source_path, &source);

        let diag = Diagnostic::error()
            .with_message(format!("{} is missing", declaration_kind))
            .with_labels(vec![Label::primary(file_id, start..end).with_message(
                format!(
                    "Module '{}' expected {} '{}', but found none",
                    declaration_kind, module_name, identifier_name
                ),
            )])
            .with_notes(vec![format!(
                "The {} is missing in the new module, add the previously defined: '{}'",
                declaration_kind, identifier_name
            )]);

        let mut writer = StandardStream::stderr(ColorChoice::Always);
        let config = codespan_reporting::term::Config::default();

        codespan_reporting::term::emit(&mut writer, &config, &files, &diag)
            .context("Unable to print error")
    }
}

impl UpgradeCompatibilityModeError {
    /// check if the error breaks compatibility for a given [`Compatibility`]
    fn breaks_compatibility(&self, compatability: &Compatibility) -> bool {
        match self {
            UpgradeCompatibilityModeError::StructAbilityMismatch { .. }
            | UpgradeCompatibilityModeError::StructTypeParamMismatch { .. }
            | UpgradeCompatibilityModeError::EnumAbilityMismatch { .. }
            | UpgradeCompatibilityModeError::EnumTypeParamMismatch { .. }
            | UpgradeCompatibilityModeError::FunctionMissingPublic { .. }
            | UpgradeCompatibilityModeError::FunctionLostPublicVisibility { .. } => true,

            UpgradeCompatibilityModeError::StructFieldMismatch { .. }
            | UpgradeCompatibilityModeError::EnumVariantMissing { .. }
            | UpgradeCompatibilityModeError::EnumVariantMismatch { .. } => {
                compatability.check_datatype_layout
            }

            UpgradeCompatibilityModeError::StructMissing { .. }
            | UpgradeCompatibilityModeError::EnumMissing { .. } => true,

            UpgradeCompatibilityModeError::FunctionSignatureMismatch { old_function, .. } => {
                if old_function.visibility == Visibility::Public {
                    return true;
                }
                if old_function.is_entry {
                    compatability.check_private_entry_linking
                } else {
                    false
                }
            }

            UpgradeCompatibilityModeError::FunctionMissingEntry { .. }
            | UpgradeCompatibilityModeError::FunctionEntryCompatibility { .. } => {
                compatability.check_private_entry_linking
            }
            UpgradeCompatibilityModeError::EnumNewVariant { .. } => {
                compatability.check_datatype_layout
            }
        }
    }
}

/// A compatibility mode that collects errors as a vector of enums which describe the error causes
#[derive(Default)]
pub(crate) struct CliCompatibilityMode {
    errors: UpgradeErrorList,
}

impl CompatibilityMode for CliCompatibilityMode {
    type Error = UpgradeErrorList;
    // ignored, address is not populated pre-tx
    fn module_id_mismatch(
        &mut self,
        _old_addr: &AccountAddress,
        _old_name: &IdentStr,
        _new_addr: &AccountAddress,
        _new_name: &IdentStr,
    ) {
    }

    fn struct_missing(&mut self, name: &Identifier, old_struct: &Struct) {
        self.errors
            .push(UpgradeCompatibilityModeError::StructMissing {
                name: name.clone(),
                old_struct: old_struct.clone(),
            });
    }

    fn struct_ability_mismatch(
        &mut self,
        name: &Identifier,
        old_struct: &Struct,
        new_struct: &Struct,
    ) {
        self.errors
            .push(UpgradeCompatibilityModeError::StructAbilityMismatch {
                name: name.clone(),
                old_struct: old_struct.clone(),
                new_struct: new_struct.clone(),
            });
    }

    fn struct_type_param_mismatch(
        &mut self,
        name: &Identifier,
        old_struct: &Struct,
        new_struct: &Struct,
    ) {
        self.errors
            .push(UpgradeCompatibilityModeError::StructTypeParamMismatch {
                name: name.clone(),
                old_struct: old_struct.clone(),
                new_struct: new_struct.clone(),
            });
    }

    fn struct_field_mismatch(
        &mut self,
        name: &Identifier,
        old_struct: &Struct,
        new_struct: &Struct,
    ) {
        self.errors
            .push(UpgradeCompatibilityModeError::StructFieldMismatch {
                name: name.clone(),
                old_struct: old_struct.clone(),
                new_struct: new_struct.clone(),
            });
    }

    fn enum_missing(&mut self, name: &Identifier, old_enum: &Enum) {
        self.errors
            .push(UpgradeCompatibilityModeError::EnumMissing {
                name: name.clone(),
                old_enum: old_enum.clone(),
            });
    }

    fn enum_ability_mismatch(&mut self, name: &Identifier, old_enum: &Enum, new_enum: &Enum) {
        self.errors
            .push(UpgradeCompatibilityModeError::EnumAbilityMismatch {
                name: name.clone(),
                old_enum: old_enum.clone(),
                new_enum: new_enum.clone(),
            });
    }

    fn enum_type_param_mismatch(&mut self, name: &Identifier, old_enum: &Enum, new_enum: &Enum) {
        self.errors
            .push(UpgradeCompatibilityModeError::EnumTypeParamMismatch {
                name: name.clone(),
                old_enum: old_enum.clone(),
                new_enum: new_enum.clone(),
            });
    }

    fn enum_new_variant(&mut self, name: &Identifier, old_enum: &Enum, new_enum: &Enum) {
        self.errors
            .push(UpgradeCompatibilityModeError::EnumNewVariant {
                name: name.clone(),
                old_enum: old_enum.clone(),
                new_enum: new_enum.clone(),
            });
    }

    fn enum_variant_missing(&mut self, name: &Identifier, old_enum: &Enum, tag: usize) {
        self.errors
            .push(UpgradeCompatibilityModeError::EnumVariantMissing {
                name: name.clone(),
                old_enum: old_enum.clone(),
                tag,
            });
    }

    fn enum_variant_mismatch(
        &mut self,
        name: &Identifier,
        old_enum: &Enum,
        new_enum: &Enum,
        variant_idx: usize,
    ) {
        self.errors
            .push(UpgradeCompatibilityModeError::EnumVariantMismatch {
                name: name.clone(),
                old_enum: old_enum.clone(),
                new_enum: new_enum.clone(),
                tag: variant_idx,
            });
    }

    fn function_missing_public(&mut self, name: &Identifier, old_function: &Function) {
        self.errors
            .push(UpgradeCompatibilityModeError::FunctionMissingPublic {
                name: name.clone(),
                old_function: old_function.clone(),
            });
    }

    fn function_missing_entry(&mut self, name: &Identifier, old_function: &Function) {
        self.errors
            .push(UpgradeCompatibilityModeError::FunctionMissingEntry {
                name: name.clone(),
                old_function: old_function.clone(),
            });
    }

    fn function_signature_mismatch(
        &mut self,
        name: &Identifier,
        old_function: &Function,
        new_function: &Function,
    ) {
        self.errors
            .push(UpgradeCompatibilityModeError::FunctionSignatureMismatch {
                name: name.clone(),
                old_function: old_function.clone(),
                new_function: new_function.clone(),
            });
    }

    fn function_lost_public_visibility(&mut self, name: &Identifier, old_function: &Function) {
        self.errors.push(
            UpgradeCompatibilityModeError::FunctionLostPublicVisibility {
                name: name.clone(),
                old_function: old_function.clone(),
            },
        );
    }

    fn function_entry_compatibility(
        &mut self,
        name: &Identifier,
        old_function: &Function,
        new_function: &Function,
    ) {
        self.errors
            .push(UpgradeCompatibilityModeError::FunctionEntryCompatibility {
                name: name.clone(),
                old_function: old_function.clone(),
                new_function: new_function.clone(),
            });
    }

    fn finish(&mut self, compatability: &Compatibility) -> Result<(), Self::Error> {
        self.errors.retain_incompatible(compatability);

        if !self.errors.errors.is_empty() {
            return Err(self.errors.clone());
        }

        Ok(())
    }
}

/// Check the upgrade compatibility of a new package with an existing on-chain package.
pub(crate) async fn check_compatibility(
    client: &SuiClient,
    package_id: ObjectID,
    compiled_modules: &[Vec<u8>],
    upgrade_package: CompiledPackage,
    protocol_config: ProtocolConfig,
) -> Result<(), Error> {
    let new_modules = compiled_modules
        .iter()
        .map(|b| CompiledModule::deserialize_with_config(b, &to_binary_config(&protocol_config)))
        .collect::<Result<Vec<_>, _>>()
        .context("Unable to to deserialize compiled module")?;

    let existing_obj_read = client
        .read_api()
        .get_object_with_options(package_id, SuiObjectDataOptions::new().with_bcs())
        .await
        .context("Unable to get existing package")?;

    let existing_obj = existing_obj_read
        .into_object()
        .context("Unable to get existing package")?
        .bcs
        .ok_or_else(|| anyhow!("Unable to read object"))?;

    let existing_package = match existing_obj {
        SuiRawData::Package(pkg) => Ok(pkg),
        SuiRawData::MoveObject(_) => Err(anyhow!("Object found when package expected")),
    }?;

    let existing_modules = existing_package
        .module_map
        .iter()
        .map(|m| CompiledModule::deserialize_with_config(m.1, &to_binary_config(&protocol_config)))
        .collect::<Result<Vec<_>, _>>()
        .context("Unable to get existing package")?;

    compare_packages(existing_modules, upgrade_package, new_modules)
}

/// Collect all the errors into a single error message.
fn compare_packages(
    existing_modules: Vec<CompiledModule>,
    upgrade_package: CompiledPackage,
    new_modules: Vec<CompiledModule>,
) -> Result<(), Error> {
    // create a map from the new modules
    let new_modules_map: HashMap<Identifier, CompiledModule> = new_modules
        .iter()
        .map(|m| (m.self_id().name().to_owned(), m.clone()))
        .collect();

    let errors: Vec<String> = existing_modules
        .iter()
        .map(|existing_module| {
            let name = existing_module.self_id().name().to_owned();

            let compiled_unit_with_source = upgrade_package
                .package
                .get_module_by_name_from_root(&name.to_string())
                .context("Unable to get module")?;

            // find the new module with the same name
            match new_modules_map.get(&name) {
                Some(new_module) => Compatibility::upgrade_check()
                    .check_with_mode::<CliCompatibilityMode>(
                        &Module::new(&existing_module),
                        &Module::new(new_module),
                    )
                    .map_err(|mut error_list| {
                        if let Err(print_errors) =
                            error_list.print_errors(&compiled_unit_with_source)
                        {
                            print_errors
                        } else {
                            anyhow!("Compatibility check failed for module '{}'", name)
                        }
                    }),
                None => Err(anyhow!("Module '{}' is missing from the package", name)),
            }
        })
        // filter to errors
        .filter(|r| r.is_err())
        // collect the errors
        .map(|r| r.unwrap_err().to_string())
        .collect();

    if errors.len() == 1 {
        return Err(anyhow!(errors[0].clone()));
    } else if !errors.is_empty() {
        return Err(anyhow!(
            "Upgrade compatibility check failed with the following errors:\n{}",
            errors
                .iter()
                .map(|e| format!("- {}", e))
                .collect::<Vec<String>>()
                .join("\n")
        ));
    }

    Ok(())
}
