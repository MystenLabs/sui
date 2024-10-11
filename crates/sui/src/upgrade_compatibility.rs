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
    ModuleMissing {
        name: Identifier,
    },
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

impl UpgradeCompatibilityModeError {
    /// check if the error breaks compatibility for a given [`Compatibility`]
    fn breaks_compatibility(&self, compatability: &Compatibility) -> bool {
        match self {
            UpgradeCompatibilityModeError::ModuleMissing { .. } => true,

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
    errors: Vec<UpgradeCompatibilityModeError>,
}

impl CompatibilityMode for CliCompatibilityMode {
    type Error = Vec<UpgradeCompatibilityModeError>;
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

    fn finish(&self, compatability: &Compatibility) -> Result<(), Self::Error> {
        let errors: Vec<UpgradeCompatibilityModeError> = self
            .errors
            .iter()
            .filter(|e| e.breaks_compatibility(compatability))
            .cloned()
            .collect();
        if !errors.is_empty() {
            return Err(errors);
        }
        Ok(())
    }
}

/// Check the upgrade compatibility of a new package with an existing on-chain package.
pub(crate) async fn check_compatibility(
    client: &SuiClient,
    package_id: ObjectID,
    new_package: CompiledPackage,
    protocol_config: ProtocolConfig,
) -> Result<(), Error> {
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

    compare_packages(existing_modules, new_package, true)
}

/// Collect all the errors into a single error message.
fn compare_packages(
    existing_modules: Vec<CompiledModule>,
    new_package: CompiledPackage,
    enable_colors: bool,
) -> Result<(), Error> {
    // create a map from the new modules
    let new_modules_map: HashMap<Identifier, CompiledModule> = new_package
        .get_modules()
        .map(|m| (m.self_id().name().to_owned(), m.clone()))
        .collect();

    let errors: Vec<(Identifier, UpgradeCompatibilityModeError)> = existing_modules
        .iter()
        .flat_map(|existing_module| {
            let name = existing_module.self_id().name().to_owned();

            // find the new module with the same name
            match new_modules_map.get(&name) {
                Some(new_module) => {
                    let compatible = Compatibility::upgrade_check()
                        .check_with_mode::<CliCompatibilityMode>(
                            &Module::new(existing_module),
                            &Module::new(new_module),
                        );
                    if let Err(errors) = compatible {
                        errors.into_iter().map(|e| (name.to_owned(), e)).collect()
                    } else {
                        vec![]
                    }
                }
                None => vec![(
                    name.clone(),
                    UpgradeCompatibilityModeError::ModuleMissing { name },
                )],
            }
        })
        .collect();

    if errors.is_empty() {
        return Ok(());
    }

    let mut files = SimpleFiles::new();
    let config = codespan_reporting::term::Config::default();
    let mut writer;
    if enable_colors {
        writer = codespan_reporting::term::termcolor::Buffer::ansi();
    } else {
        writer = codespan_reporting::term::termcolor::Buffer::no_color();
    }
    let mut file_id_map = HashMap::new();

    for (name, err) in errors {
        let compiled_unit_with_source = new_package
            .package
            .get_module_by_name_from_root(&name.to_string())
            .context("Unable to get module")?;

        let source_path = compiled_unit_with_source.source_path.to_string_lossy();
        let file_id = match file_id_map.get(&source_path) {
            Some(file_id) => *file_id,
            None => {
                let source = fs::read_to_string(&compiled_unit_with_source.source_path)
                    .context("Unable to read source file")?;
                let file_id = files.add(source_path.clone(), source);
                file_id_map.insert(source_path.clone(), file_id);
                file_id
            }
        };

        codespan_reporting::term::emit(
            &mut writer,
            &config,
            &files,
            &diag_from_error(err, compiled_unit_with_source, file_id),
        )?;
    }

    Err(anyhow!(
        "Upgrade compatibility check failed:\n{}",
        String::from_utf8(writer.into_inner()).context("Unable to convert buffer to string")?
    ))
}

/// Convert an error to a diagnostic using the specific error type's function.
fn diag_from_error(
    error: UpgradeCompatibilityModeError,
    compiled_unit_with_source: &CompiledUnitWithSource,
    file_id: usize,
) -> Diagnostic<usize> {
    match error {
        UpgradeCompatibilityModeError::StructMissing { name, .. } => missing_definition_diag(
            "struct",
            name.to_string(),
            compiled_unit_with_source,
            file_id,
        ),
        UpgradeCompatibilityModeError::EnumMissing { name, .. } => {
            missing_definition_diag("enum", name.to_string(), compiled_unit_with_source, file_id)
        }
        UpgradeCompatibilityModeError::FunctionMissingPublic { name, .. } => {
            missing_definition_diag(
                "public function",
                name.to_string(),
                compiled_unit_with_source,
                file_id,
            )
        }
        UpgradeCompatibilityModeError::FunctionMissingEntry { name, .. } => {
            missing_definition_diag(
                "entry function",
                name.to_string(),
                compiled_unit_with_source,
                file_id,
            )
        }
        _ => todo!("Implement diag_from_error for {:?}", error),
    }
}

/// Return a diagnostic for a missing definition.
fn missing_definition_diag(
    declaration_kind: &str,
    identifier_name: String,
    compiled_unit_with_source: &CompiledUnitWithSource,
    file_id: usize,
) -> Diagnostic<usize> {
    let module_name = compiled_unit_with_source.unit.name;

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

    Diagnostic::error()
        .with_message(format!("{declaration_kind} is missing"))
        .with_labels(vec![Label::primary(file_id, start..end).with_message(
            format!(
                "Module '{module_name}' expected {declaration_kind} '{identifier_name}', but found none"
            ),
        )])
        .with_notes(vec![format!(
            "The {declaration_kind} is missing in the new module, add the previously defined: '{identifier_name}'"
        )])
}
