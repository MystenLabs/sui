// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[path = "unit_tests/upgrade_compatibility_tests.rs"]
#[cfg(test)]
mod upgrade_compatibility_tests;

use anyhow::{anyhow, Context, Error};
use regex::Regex;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use move_binary_format::compatibility::InclusionCheck;
use move_binary_format::file_format::{
    AbilitySet, DatatypeTyParameter, EnumDefinitionIndex, FunctionDefinitionIndex,
    StructDefinitionIndex, TableIndex,
};
use move_binary_format::normalized::Type;
use move_binary_format::{
    compatibility::Compatibility,
    compatibility_mode::CompatibilityMode,
    file_format::Visibility,
    normalized::{Enum, Field, Function, Module, Struct, Type, Variant},
    CompiledModule,
};
use move_bytecode_source_map::source_map::SourceName;
use move_command_line_common::files::FileHash;
use move_compiler::diagnostics::codes::DiagnosticInfo;
use move_compiler::{
    diagnostics::{
        codes::{custom, Severity},
        report_diagnostics_to_buffer, Diagnostic, Diagnostics,
    },
    shared::files::{FileName, FilesSourceText},
};
use move_core_types::{
    account_address::AccountAddress,
    identifier::{IdentStr, Identifier},
};
use move_ir_types::location::{ByteIndex, Loc};
use move_package::compilation::compiled_package::CompiledUnitWithSource;
use sui_json_rpc_types::{SuiObjectDataOptions, SuiRawData};
use sui_move_build::CompiledPackage;
use sui_protocol_config::ProtocolConfig;
use sui_sdk::SuiClient;
use sui_types::move_package::UpgradePolicy;
use sui_types::{base_types::ObjectID, execution_config_utils::to_binary_config};

/// Errors that can occur during upgrade compatibility checks.
/// one-to-one related to the underlying trait functions see: [`CompatibilityMode`]
/// Except for the `ModuleMismatch` which is a special case for additive and dependency only policies and `ModuleMissing`
#[derive(Debug, Clone)]
pub(crate) enum UpgradeCompatibilityModeError {
    ModuleMissing {
        name: Identifier,
    },
    /// The upgrade is not compatible with the existing package due to the policy.
    /// This error is used for additive and dependency only policies where modules
    /// are either not allowed to add declarations or change them.
    ModuleMismatch {
        policy: UpgradePolicy,
    },
    StructMissing {
        name: Identifier,
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
    },
    FunctionMissingPublic {
        name: Identifier,
    },
    FunctionMissingEntry {
        name: Identifier,
    },
    FunctionSignatureMismatch {
        name: Identifier,
        old_function: Function,
        new_function: Function,
    },
    FunctionLostPublicVisibility {
        name: Identifier,
    },
    FunctionEntryCompatibility {
        name: Identifier,
        old_function: Function,
    },
}

impl UpgradeCompatibilityModeError {
    /// check if the error breaks compatibility for a given [`Compatibility`]
    fn breaks_compatibility(&self, compatability: &Compatibility) -> bool {
        match self {
            UpgradeCompatibilityModeError::ModuleMissing { .. }
            | UpgradeCompatibilityModeError::ModuleMismatch { .. } => true,

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

    fn struct_missing(&mut self, name: &Identifier, _old_struct: &Struct) {
        self.errors
            .push(UpgradeCompatibilityModeError::StructMissing { name: name.clone() });
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

    fn enum_missing(&mut self, name: &Identifier, _old_enum: &Enum) {
        self.errors
            .push(UpgradeCompatibilityModeError::EnumMissing { name: name.clone() });
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
        _variant_idx: usize,
    ) {
        self.errors
            .push(UpgradeCompatibilityModeError::EnumVariantMismatch {
                name: name.clone(),
                old_enum: old_enum.clone(),
                new_enum: new_enum.clone(),
            });
    }

    fn function_missing_public(&mut self, name: &Identifier, _old_function: &Function) {
        self.errors
            .push(UpgradeCompatibilityModeError::FunctionMissingPublic { name: name.clone() });
    }

    fn function_missing_entry(&mut self, name: &Identifier, _old_function: &Function) {
        self.errors
            .push(UpgradeCompatibilityModeError::FunctionMissingEntry { name: name.clone() });
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

    fn function_lost_public_visibility(&mut self, name: &Identifier, _old_function: &Function) {
        self.errors.push(
            UpgradeCompatibilityModeError::FunctionLostPublicVisibility { name: name.clone() },
        );
    }

    fn function_entry_compatibility(
        &mut self,
        name: &Identifier,
        old_function: &Function,
        _new_function: &Function,
    ) {
        self.errors
            .push(UpgradeCompatibilityModeError::FunctionEntryCompatibility {
                name: name.clone(),
                old_function: old_function.clone(),
            });
    }

    fn finish(self, compatability: &Compatibility) -> Result<(), Self::Error> {
        let errors: Vec<UpgradeCompatibilityModeError> = self
            .errors
            .into_iter()
            .filter(|e| e.breaks_compatibility(compatability))
            .collect();

        if !errors.is_empty() {
            return Err(errors);
        }
        Ok(())
    }
}

struct IdentifierTableLookup {
    struct_identifier_to_index: BTreeMap<Identifier, TableIndex>,
    enum_identifier_to_index: BTreeMap<Identifier, TableIndex>,
    function_identifier_to_index: BTreeMap<Identifier, TableIndex>,
}

/// creates an index to allow looking up the table index of a struct, enum, or function by its identifier
fn table_index(compiled_module: &CompiledModule) -> IdentifierTableLookup {
    // for each in compiled module
    let struct_identifier_to_index: BTreeMap<Identifier, TableIndex> = compiled_module
        .struct_defs()
        .iter()
        .enumerate()
        .map(|(i, d)| {
            // get the identifier of the struct
            let s_id = compiled_module
                .identifier_at(compiled_module.datatype_handle_at(d.struct_handle).name);
            (s_id.to_owned(), i as TableIndex)
        })
        .collect();

    let enum_identifier_to_index: BTreeMap<Identifier, TableIndex> = compiled_module
        .enum_defs()
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let e_id = compiled_module
                .identifier_at(compiled_module.datatype_handle_at(d.enum_handle).name);
            (e_id.to_owned(), i as TableIndex)
        })
        .collect();

    let function_identifier_to_index: BTreeMap<Identifier, TableIndex> = compiled_module
        .function_defs()
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let f_id =
                compiled_module.identifier_at(compiled_module.function_handle_at(d.function).name);
            (f_id.to_owned(), i as TableIndex)
        })
        .collect();

    IdentifierTableLookup {
        struct_identifier_to_index,
        enum_identifier_to_index,
        function_identifier_to_index,
    }
}

const COMPATIBILITY_PREFIX: &str = "Compatibility ";
/// Generates an enum Category along with individual enum for each individual category
/// and impls into diagnostic info for each category.
macro_rules! upgrade_codes {
    ($($cat:ident: [
        $($code:ident: { msg: $code_msg:literal }),* $(,)?
    ]),* $(,)?) => {
        #[derive(PartialEq, Eq, Clone, Copy, Debug, Hash, PartialOrd, Ord)]
        #[repr(u8)]
        pub enum Category {
            #[allow(dead_code)]
            ZeroPlaceholder,
            $($cat,)*
        }

        $(
            #[derive(PartialEq, Eq, Clone, Copy, Debug, Hash)]
            #[repr(u8)]
            pub enum $cat {
                #[allow(dead_code)]
                ZeroPlaceholder,
                $($code,)*
            }

            #[allow(clippy::from_over_into)]
            impl Into<DiagnosticInfo> for $cat {
                fn into(self) -> DiagnosticInfo {
                    match self {
                        Self::ZeroPlaceholder =>
                            panic!("do not use placeholder error code"),
                        $(Self::$code => custom(
                            COMPATIBILITY_PREFIX,
                            Severity::NonblockingError,
                            Category::$cat as u8,
                            self as u8,
                            $code_msg,
                        ),)*
                    }
                }
            }
        )*
    };
}

// Used to generate diagnostics primary labels for upgrade compatibility errors.
// WARNING: you should add new codes to the END of each category list to avoid breaking the existing codes.
// adding into the middle of a list will change the error code numbers "error[Compatibility EXXXXX]"
// similarly new categories should be added to the end of the outer list.
upgrade_codes!(
    Declarations: [
        PublicMissing: { msg: "missing public declaration" },
        TypeMismatch: { msg: "type mismatch" },
        AbilityMismatch: { msg: "ability mismatch" },
        FieldMismatch: { msg: "field mismatch" },
        TypeParamMismatch: { msg: "type parameter mismatch" },
        ModuleMismatch: { msg: "module incompatible" },
        ModuleMissing: { msg: "module missing" },
    ],
    Enums: [
        VariantMismatch: { msg: "variant mismatch" },
    ],
    Functions_: [
        SignatureMismatch: { msg: "function signature mismatch" },
        EntryMismatch: { msg: "function entry mismatch" },
    ],
);

/// Check the upgrade compatibility of a new package with an existing on-chain package.
pub(crate) async fn check_compatibility(
    client: &SuiClient,
    package_id: ObjectID,
    new_package: CompiledPackage,
    package_path: PathBuf,
    upgrade_policy: u8,
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

    let policy =
        UpgradePolicy::try_from(upgrade_policy).map_err(|_| anyhow!("Invalid upgrade policy"))?;

    compare_packages(existing_modules, new_package, package_path, policy)
}

fn compare_packages(
    existing_modules: Vec<CompiledModule>,
    mut new_package: CompiledPackage,
    package_path: PathBuf,
    policy: UpgradePolicy,
) -> Result<(), Error> {
    let new_modules_map: HashMap<Identifier, CompiledModule> = new_package
        .get_modules()
        .map(|m| (m.self_id().name().to_owned(), m.clone()))
        .collect();

    let lookup: HashMap<Identifier, IdentifierTableLookup> = new_package
        .get_modules()
        .map(|m| (m.self_id().name().to_owned(), table_index(m)))
        .collect();

    let errors: Vec<(Identifier, UpgradeCompatibilityModeError)> = existing_modules
        .iter()
        .flat_map(|existing_module| {
            let name = existing_module.self_id().name().to_owned();

            // find the new module with the same name
            match new_modules_map.get(&name) {
                Some(new_module) => {
                    let existing_module = Module::new(existing_module);
                    let new_module = Module::new(new_module);

                    match policy {
                        UpgradePolicy::Compatible => errors_or_empty_vec(
                            name,
                            Compatibility::upgrade_check().check_with_mode::<CliCompatibilityMode>(
                                &existing_module,
                                &new_module,
                            ),
                        ),
                        // TODO improve on this error message
                        UpgradePolicy::Additive => errors_or_empty_vec(
                            name,
                            InclusionCheck::Subset
                                .check(&existing_module, &new_module)
                                .map_err(|_| {
                                    vec![UpgradeCompatibilityModeError::ModuleMismatch {
                                        policy: UpgradePolicy::Additive,
                                    }]
                                }),
                        ),
                        // TODO improve on this error message
                        UpgradePolicy::DepOnly => errors_or_empty_vec(
                            name,
                            InclusionCheck::Equal
                                .check(&existing_module, &new_module)
                                .map_err(|_| {
                                    vec![UpgradeCompatibilityModeError::ModuleMismatch {
                                        policy: UpgradePolicy::DepOnly,
                                    }]
                                }),
                        ),
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

    let mut diags = Diagnostics::new();

    // add move toml
    let move_toml_path = package_path.join("Move.toml");
    let move_toml_contents = Arc::from(
        fs::read_to_string(&move_toml_path)
            .context("Unable to read Move.toml")?
            .to_string(),
    );
    let move_toml_hash = FileHash::new(&move_toml_contents);

    new_package.package.file_map.add(
        FileHash::new(&move_toml_contents),
        FileName::from(move_toml_path.to_string_lossy()),
        Arc::clone(&move_toml_contents),
    );

    for (name, err) in errors {
        if let UpgradeCompatibilityModeError::ModuleMissing { name, .. } = &err {
            diags.extend(missing_module_diag(
                name,
                &move_toml_hash,
                &move_toml_contents,
            )?);
            continue;
        }

        let compiled_unit_with_source = new_package
            .package
            .get_module_by_name_from_root(name.as_str())
            .context("Unable to get module")?;

        diags.extend(diag_from_error(
            &err,
            compiled_unit_with_source,
            &lookup[&name],
        )?);
    }

    // use colors but inline
    Err(anyhow!(
        "{}\nUpgrade failed, this package requires changes to be compatible with the existing package. \
        Its upgrade policy is set to '{}'.",
        if !diags.is_empty() {
            String::from_utf8(report_diagnostics_to_buffer(
                &new_package.package.file_map,
                diags,
                use_colors()
            ))
            .context("Unable to convert buffer to string")?
        } else {
            "".to_string()
        },
        match policy {
            UpgradePolicy::Compatible => "compatible",
            UpgradePolicy::Additive => "additive",
            UpgradePolicy::DepOnly => "dependency only",
        }
    ))
}

fn errors_or_empty_vec(
    name: Identifier,
    result: Result<(), Vec<UpgradeCompatibilityModeError>>,
) -> Vec<(Identifier, UpgradeCompatibilityModeError)> {
    match result {
        Ok(_) => vec![],
        Err(errors) => errors.into_iter().map(|e| (name.clone(), e)).collect(),
    }
}

/// Convert an error to a diagnostic using the specific error type's function.
fn diag_from_error(
    error: &UpgradeCompatibilityModeError,
    compiled_unit_with_source: &CompiledUnitWithSource,
    lookup: &IdentifierTableLookup,
) -> Result<Diagnostics, Error> {
    match error {
        UpgradeCompatibilityModeError::StructMissing { name, .. } => {
            missing_definition_diag("struct", name, compiled_unit_with_source)
        }
        UpgradeCompatibilityModeError::StructAbilityMismatch {
            name,
            old_struct,
            new_struct,
        } => struct_ability_mismatch_diag(
            name,
            old_struct,
            new_struct,
            compiled_unit_with_source,
            lookup,
        ),
        UpgradeCompatibilityModeError::StructFieldMismatch {
            name,
            old_struct,
            new_struct,
        } => struct_field_mismatch_diag(
            name,
            old_struct,
            new_struct,
            compiled_unit_with_source,
            lookup,
        ),

        UpgradeCompatibilityModeError::StructTypeParamMismatch {
            name,
            old_struct,
            new_struct,
        } => struct_type_param_mismatch_diag(
            name,
            old_struct,
            new_struct,
            compiled_unit_with_source,
            lookup,
        ),

        UpgradeCompatibilityModeError::EnumMissing { name, .. } => {
            missing_definition_diag("enum", name, compiled_unit_with_source)
        }
        UpgradeCompatibilityModeError::EnumAbilityMismatch {
            name,
            old_enum,
            new_enum,
        } => {
            enum_ability_mismatch_diag(name, old_enum, new_enum, compiled_unit_with_source, lookup)
        }
        UpgradeCompatibilityModeError::EnumNewVariant {
            name,
            old_enum,
            new_enum,
        } => enum_new_variant_diag(
            name,
            old_enum,
            new_enum,
            // *tag,
            compiled_unit_with_source,
            lookup,
        ),
        UpgradeCompatibilityModeError::EnumVariantMissing {
            name,
            tag,
            old_enum,
        } => enum_variant_missing_diag(name, old_enum, *tag, compiled_unit_with_source, lookup),
        UpgradeCompatibilityModeError::EnumVariantMismatch {
            name,
            old_enum,
            new_enum,
            ..
        } => {
            enum_variant_mismatch_diag(name, old_enum, new_enum, compiled_unit_with_source, lookup)
        }
        UpgradeCompatibilityModeError::EnumTypeParamMismatch {
            name,
            old_enum,
            new_enum,
        } => enum_type_param_mismatch(name, old_enum, new_enum, compiled_unit_with_source, lookup),
        UpgradeCompatibilityModeError::FunctionMissingPublic { name, .. } => {
            missing_definition_diag("public function", name, compiled_unit_with_source)
        }
        UpgradeCompatibilityModeError::FunctionMissingEntry { name, .. } => {
            missing_definition_diag("entry function", name, compiled_unit_with_source)
        }
        UpgradeCompatibilityModeError::FunctionLostPublicVisibility { name, .. } => {
            function_lost_public(name, compiled_unit_with_source, lookup)
        }
        UpgradeCompatibilityModeError::FunctionSignatureMismatch {
            name,
            old_function,
            new_function,
        } => function_signature_mismatch_diag(
            name,
            old_function,
            new_function,
            compiled_unit_with_source,
            lookup,
        ),

        UpgradeCompatibilityModeError::FunctionEntryCompatibility {
            name, old_function, ..
        } => function_entry_mismatch(name, old_function, compiled_unit_with_source, lookup),
        // Specifically handles additive and dep only policies where modules
        // are either not allowed to add declarations or change them.
        UpgradeCompatibilityModeError::ModuleMismatch { policy } => {
            module_compatibility_error_diag(*policy, compiled_unit_with_source)
        }
        UpgradeCompatibilityModeError::ModuleMissing { .. } => {
            unreachable!("Module Missing should be handled by outer function")
        }
    }
}

// TODO provide more depth in the diagnostics
// give specifics about the declarations which do not match
fn module_compatibility_error_diag(
    policy: UpgradePolicy,
    compiled_unit_with_source: &CompiledUnitWithSource,
) -> Result<Diagnostics, Error> {
    let mut diags = Diagnostics::new();

    let loc = compiled_unit_with_source
        .unit
        .source_map
        .definition_location;

    diags.add(Diagnostic::new(
        Declarations::ModuleMismatch,
        (
            loc,
            format!(
                "The upgrade is not compatible with the existing package due to {} policy.",
                match policy {
                    UpgradePolicy::Additive => "additive",
                    UpgradePolicy::DepOnly => "dependency only",
                    _ => unreachable!("Invalid upgrade policy for this error type"),
                }
            ),
        ),
        Vec::<(Loc, String)>::new(),
        vec![
            "The upgrade is not compatible with the existing package.".to_string(),
            format!(
                "The upgrade policy is set to '{}'.",
                policy.to_string().to_lowercase()
            ),
        ],
    ));

    Ok(diags)
}

fn missing_module_diag(
    module_name: &Identifier,
    move_toml_hash: &FileHash,
    move_toml_contents: &Arc<str>,
) -> Result<Diagnostics, Error> {
    const PACKAGE_TABLE: &str = "[package]";
    let mut diags = Diagnostics::new();

    let start: usize = move_toml_contents.find(PACKAGE_TABLE).unwrap_or_default();
    // default to the end of the package table definition
    // get the third newline after the start of the package table declaration if it exists
    let end = move_toml_contents[start..]
        .match_indices('\n')
        .take(3)
        .last()
        .map(|(idx, _)| start + idx)
        .unwrap_or(start + PACKAGE_TABLE.len());

    let loc = Loc::new(move_toml_hash.clone(), start as ByteIndex, end as ByteIndex);

    diags.add(Diagnostic::new(
        Declarations::ModuleMissing,
        (loc, format!("Package is missing module '{module_name}'",)),
        Vec::<(Loc, String)>::new(),
        vec![
            "Modules which are part package cannot be removed during an upgrade.".to_string(),
            format!("add missing module '{module_name}' back to the package."),
        ],
    ));

    Ok(diags)
}

/// Return a diagnostic for a missing definition.
fn missing_definition_diag(
    declaration_kind: &str,
    identifier_name: &Identifier,
    compiled_unit_with_source: &CompiledUnitWithSource,
) -> Result<Diagnostics, Error> {
    let mut diags = Diagnostics::new();

    let module_name = compiled_unit_with_source.unit.name;
    let loc = compiled_unit_with_source
        .unit
        .source_map
        .definition_location;

    diags.add(Diagnostic::new(
        Declarations::PublicMissing,
        (
            loc,
            format!(
                "{declaration_kind} '{identifier_name}' is missing",
                declaration_kind = declaration_kind,
                identifier_name = identifier_name,
            ),
        ),
        std::iter::empty::<(Loc, String)>(),
        vec![
            format!(
                "{declaration_kind}s are part of a module's public interface \
                and cannot be removed or changed during an upgrade.",
            ),
            format!(
                "add missing {declaration_kind} '{identifier_name}' \
                back to the module '{module_name}'.",
            ),
        ],
    ));

    Ok(diags)
}

/// Return a diagnostic for a function which has lost its public visibility
fn function_lost_public(
    function_name: &Identifier,
    compiled_unit_with_source: &CompiledUnitWithSource,
    lookup: &IdentifierTableLookup,
) -> Result<Diagnostics, Error> {
    let mut diags = Diagnostics::new();

    let func_index = lookup
        .function_identifier_to_index
        .get(function_name)
        .context("Unable to get function index")?;

    let func_sourcemap = compiled_unit_with_source
        .unit
        .source_map
        .get_function_source_map(FunctionDefinitionIndex::new(*func_index))
        .context("Unable to get function source map")?;

    let def_loc = func_sourcemap.definition_location;

    diags.add(Diagnostic::new(
        Declarations::PublicMissing,
        (
            def_loc,
            format!("Function '{function_name}' has lost its public visibility",),
        ),
        Vec::<(Loc, String)>::new(),
        vec![
            "Functions are part of a module's public interface \
            and cannot be changed during an upgrade."
                .to_string(),
            format!(
                "Restore the original function's 'public' visibility for \
                function '{function_name}'.",
            ),
        ],
    ));

    Ok(diags)
}

fn format_param(
    param: &Type,
    type_params: Vec<SourceName>,
    secondary: &mut Vec<(Loc, String)>,
) -> Result<String, Error> {
    Ok(match param {
        Type::TypeParameter(t) => {
            let type_param = type_params
                .get(*t as usize)
                .context("Unable to get type param location")?;

            secondary.push((
                type_param.1,
                format!("Type parameter '{}' is defined here", &type_param.0),
            ));
            type_param.0.to_string()
        }
        Type::Vector(t) => {
            format!("vector<{}>", format_param(t, type_params, secondary)?)
        }
        Type::MutableReference(t) => {
            format!("&mut {}", format_param(t, type_params, secondary)?)
        }
        Type::Reference(t) => {
            format!("&{}", format_param(t, type_params, secondary)?)
        }
        _ => format!("{}", param),
    })
}

/// Return a diagnostic for a function signature mismatch.
/// start by checking the lengths of the parameters and returns and return a diagnostic if they are different
/// if the lengths are the same check each parameter piece wise and return a diagnostic for each mismatch
fn function_signature_mismatch_diag(
    function_name: &Identifier,
    old_function: &Function,
    new_function: &Function,
    compiled_unit_with_source: &CompiledUnitWithSource,
    lookup: &IdentifierTableLookup,
) -> Result<Diagnostics, Error> {
    let mut diags = Diagnostics::new();

    let func_index = lookup
        .function_identifier_to_index
        .get(function_name)
        .context("Unable to get function index")?;

    let func_sourcemap = compiled_unit_with_source
        .unit
        .source_map
        .get_function_source_map(FunctionDefinitionIndex::new(*func_index))
        .context("Unable to get function source map")?;

    let def_loc = func_sourcemap.definition_location;

    // handle function arguments
    if old_function.parameters.len() != new_function.parameters.len() {
        diags.add(Diagnostic::new(
            Functions_::SignatureMismatch,
            (
                def_loc,
                format!(
                    "Expected {} {}, found {}",
                    old_function.parameters.len(),
                    singular_or_plural(old_function.parameters.len(), "parameter", "parameters"),
                    new_function.parameters.len(),
                ),
            ),
            Vec::<(Loc, String)>::new(),
            vec![
                "Functions are part of a module's public interface and cannot be \
                changed during an upgrade."
                    .to_string(),
                format!(
                    "Restore the original function's {} for \
                    function '{function_name}', expected {} {}.",
                    singular_or_plural(old_function.parameters.len(), "parameter", "parameters"),
                    old_function.parameters.len(),
                    singular_or_plural(old_function.parameters.len(), "parameter", "parameters"),
                ),
            ],
        ));
    } else if old_function.parameters != new_function.parameters {
        for ((i, old_param), new_param) in old_function
            .parameters
            .iter()
            .enumerate()
            .zip(new_function.parameters.iter())
        {
            if old_param != new_param {
                let param_loc = func_sourcemap
                    .parameters
                    .get(i)
                    .context("Unable to get parameter location")?
                    .1;

                let mut secondary = Vec::new();

                let old_param = format_param(
                    old_param,
                    func_sourcemap.type_parameters.clone(),
                    &mut secondary,
                )?;
                let new_param = format_param(
                    new_param,
                    func_sourcemap.type_parameters.clone(),
                    &mut Vec::new(),
                )?;

                let label = format!("Unexpected parameter '{new_param}', expected '{old_param}'");

                diags.add(Diagnostic::new(
                    Functions_::SignatureMismatch,
                    (param_loc, label),
                    secondary,
                    vec![
                        "Functions are part of a module's public interface \
                        and cannot be changed during an upgrade."
                            .to_string(),
                        format!(
                            "Restore the original function's {} \
                            for function '{function_name}'.",
                            singular_or_plural(
                                old_function.parameters.len(),
                                "parameter",
                                "parameters"
                            )
                        ),
                    ],
                ));
            }
        }
    }
    // type parameters are a vector of AbilitySet and therefore cannot share the same logic as structs and enums
    if old_function.type_parameters.len() != new_function.type_parameters.len() {
        diags.add(Diagnostic::new(
            Declarations::TypeParamMismatch,
            (
                def_loc,
                format!(
                    "Expected {} type {}, found {}",
                    old_function.type_parameters.len(),
                    singular_or_plural(
                        old_function.type_parameters.len(),
                        "parameter",
                        "parameters"
                    ),
                    new_function.type_parameters.len()
                ),
            ),
            Vec::<(Loc, String)>::new(),
            vec![
                "Functions are part of a module's public interface \
                and cannot be changed during an upgrade."
                    .to_string(),
                format!(
                    "Restore the original function's type {} for \
                    function '{function_name}', expected {} type {}.",
                    singular_or_plural(
                        old_function.type_parameters.len(),
                        "parameter",
                        "parameters"
                    ),
                    old_function.type_parameters.len(),
                    singular_or_plural(
                        old_function.type_parameters.len(),
                        "parameter",
                        "parameters"
                    ),
                ),
            ],
        ));
    } else if old_function.type_parameters != new_function.type_parameters {
        for ((i, old_type_param), new_type_param) in old_function
            .type_parameters
            .iter()
            .enumerate()
            .zip(new_function.type_parameters.iter())
        {
            if old_type_param != new_type_param {
                let type_param_loc = func_sourcemap
                    .type_parameters
                    .get(i)
                    .context("Unable to get type parameter location")?
                    .1;

                diags.add(Diagnostic::new(
                    Declarations::TypeParamMismatch,
                    (
                        type_param_loc,
                        format!(
                            "Unexpected type parameter {}, expected {}",
                            format_list(
                                new_type_param
                                    .into_iter()
                                    .map(|t| format!("'{:?}'", t).to_lowercase()),
                                Some(("constraint", "constraints"))
                            ),
                            format_list(
                                old_type_param
                                    .into_iter()
                                    .map(|t| format!("'{:?}'", t).to_lowercase()),
                                None
                            ),
                        ),
                    ),
                    Vec::<(Loc, String)>::new(),
                    vec![
                        "Functions are part of a module's public interface \
                        and cannot be changed during an upgrade."
                            .to_string(),
                        format!(
                            "Restore the original function's type {} \
                            for function '{function_name}'.",
                            singular_or_plural(
                                old_function.type_parameters.len(),
                                "parameter",
                                "parameters"
                            )
                        ),
                    ],
                ));
            }
        }
    }

    // handle return
    if old_function.return_.len() != new_function.return_.len() {
        diags.add(Diagnostic::new(
            Functions_::SignatureMismatch,
            (
                def_loc,
                format!(
                    "Expected {} return {}, found {}",
                    old_function.return_.len(),
                    singular_or_plural(old_function.return_.len(), "type", "types"),
                    new_function.return_.len()
                ),
            ),
            Vec::<(Loc, String)>::new(),
            vec![
                "Functions are part of a module's public interface \
                and cannot be changed during an upgrade."
                    .to_string(),
                format!(
                    "Restore the original function's return {} \
                    for function '{function_name}'.",
                    singular_or_plural(old_function.return_.len(), "type", "types")
                ),
            ],
        ));
    } else if old_function.return_ != new_function.return_ {
        for ((i, old_return), new_return) in old_function
            .return_
            .iter()
            .enumerate()
            .zip(new_function.return_.iter())
        {
            let return_ = func_sourcemap
                .returns
                .get(i)
                .context("Unable to get return location")?;

            if old_return != new_return {
                let mut secondary = Vec::new();

                let old_return = format_param(
                    old_return,
                    func_sourcemap.type_parameters.clone(),
                    &mut secondary,
                )?;
                let new_return = format_param(
                    new_return,
                    func_sourcemap.type_parameters.clone(),
                    &mut Vec::new(),
                )?;

                let label = if new_function.return_.len() == 1 {
                    format!("Unexpected return type '{new_return}', expected '{old_return}'")
                } else {
                    format!(
                        "Unexpected return type '{new_return}' at position {i}, expected '{old_return}'"
                    )
                };

                diags.add(Diagnostic::new(
                    Functions_::SignatureMismatch,
                    (*return_, label),
                    secondary,
                    vec![
                        "Functions are part of a module's public interface \
                        and cannot be changed during an upgrade."
                            .to_string(),
                        format!(
                            "Restore the original function's return \
                            {} for function '{function_name}'.",
                            singular_or_plural(old_function.return_.len(), "type", "types")
                        ),
                    ],
                ));
            }
        }
    }

    Ok(diags)
}

fn function_entry_mismatch(
    function_name: &Identifier,
    old_function: &Function,
    compiled_unit_with_source: &CompiledUnitWithSource,
    lookup: &IdentifierTableLookup,
) -> Result<Diagnostics, Error> {
    let mut diags = Diagnostics::new();

    let func_index = lookup
        .function_identifier_to_index
        .get(function_name)
        .context("Unable to get function index")?;

    let func_sourcemap = compiled_unit_with_source
        .unit
        .source_map
        .get_function_source_map(FunctionDefinitionIndex::new(*func_index))
        .context("Unable to get function source map")?;

    let def_loc = func_sourcemap.definition_location;

    diags.add(Diagnostic::new(
        Functions_::EntryMismatch,
        (
            def_loc,
            if old_function.is_entry {
                format!("Function '{function_name}' has lost its entry visibility")
            } else {
                format!("Function '{function_name}' has gained entry visibility")
            },
        ),
        Vec::<(Loc, String)>::new(),
        vec![
            "Entry functions cannot be changed during an upgrade.".to_string(),
            format!(
                "Restore the original function's 'entry' visibility for \
                function '{function_name}'.",
            ),
        ],
    ));

    Ok(diags)
}

/// Return a label string for an ability mismatch.
fn ability_mismatch_label(
    old_abilities: AbilitySet,
    new_abilities: AbilitySet,
    singular_noun: &str,
    plural_noun: &str,
) -> String {
    let missing_abilities = old_abilities.difference(new_abilities);
    let extra_abilities = new_abilities.difference(old_abilities);

    let missing_abilities_list: Vec<String> = missing_abilities
        .into_iter()
        .map(|a| format!("'{:?}'", a).to_lowercase())
        .collect();
    let extra_abilities_list: Vec<String> = extra_abilities
        .into_iter()
        .map(|a| format!("'{:?}'", a).to_lowercase())
        .collect();

    match (
        missing_abilities != AbilitySet::EMPTY,
        extra_abilities != AbilitySet::EMPTY,
    ) {
        (true, true) => format!(
            "Mismatched {plural_noun}: missing {}, unexpected {}",
            format_list(missing_abilities_list, None),
            format_list(extra_abilities_list, None),
        ),
        (true, false) => format!(
            "Missing {}: {}",
            singular_or_plural(missing_abilities_list.len(), singular_noun, plural_noun),
            format_list(missing_abilities_list, None)
        ),
        (false, true) => format!(
            "Unexpected {}: {}",
            singular_or_plural(extra_abilities_list.len(), singular_noun, plural_noun),
            format_list(extra_abilities_list, None)
        ),
        (false, false) => unreachable!("{plural_noun} should not be the same"),
    }
}

/// Return a diagnostic for an ability mismatch.
fn struct_ability_mismatch_diag(
    struct_name: &Identifier,
    old_struct: &Struct,
    new_struct: &Struct,
    compiled_unit_with_source: &CompiledUnitWithSource,
    lookup: &IdentifierTableLookup,
) -> Result<Diagnostics, Error> {
    let mut diags = Diagnostics::new();

    let struct_index = lookup
        .struct_identifier_to_index
        .get(struct_name)
        .context("Unable to get struct index")?;

    let struct_sourcemap = compiled_unit_with_source
        .unit
        .source_map
        .get_struct_source_map(StructDefinitionIndex::new(*struct_index))
        .context("Unable to get struct source map")?;

    let def_loc = struct_sourcemap.definition_location;

    if old_struct.abilities != new_struct.abilities {
        let old_abilities: Vec<String> = old_struct
            .abilities
            .into_iter()
            .map(|a| format!("'{:?}'", a).to_lowercase())
            .collect();

        diags.add(Diagnostic::new(
            Declarations::AbilityMismatch,
            (
                def_loc,
                ability_mismatch_label(
                    old_struct.abilities,
                    new_struct.abilities,
                    "ability",
                    "abilities",
                ),
            ),
            Vec::<(Loc, String)>::new(),
            vec![
                "Structs are part of a module's public interface and \
                cannot be changed during an upgrade."
                    .to_string(),
                format!(
                    "Restore the original {} of struct '{struct_name}': {}.",
                    singular_or_plural(old_abilities.len(), "ability", "abilities"),
                    format_list(
                        old_struct
                            .abilities
                            .into_iter()
                            .map(|a| format!("'{:?}'", a).to_lowercase()),
                        None
                    ),
                ),
            ],
        ));
    }

    Ok(diags)
}

/// Return a diagnostic for an ability mismatch. returns (full version, name, type)
fn field_to_string(field: &Field) -> (String, String, String) {
    let mut field_full = format!("'{}: {}'", field.name, field.type_);
    let mut field_name = format!("'{}'", field.name);
    let field_type = format!("'{}'", field.type_);

    if let Some(pos_num) = Regex::new(r"^pos(\d)+$")
        .ok()
        .and_then(|r| r.captures(field.name.as_str()))
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<u64>().ok())
    {
        field_name = format!("at position {}", pos_num);
        field_full = format!("{} {}", field_type, field_name);
    }

    (field_full, field_name, field_type)
}

/// returns a message for the given field
fn field_mismatch_message(old_field: &Field, new_field: &Field) -> (Declarations, String) {
    let (old_field_full, old_field_name, old_field_type) = field_to_string(old_field);
    let (new_field_full, new_field_name, new_field_type) = field_to_string(new_field);

    match (
        old_field.name != new_field.name,
        old_field.type_ != new_field.type_,
    ) {
        (true, true) => (
            Declarations::FieldMismatch,
            format!(
                "Mismatched field {}, expected {}.",
                new_field_full, old_field_full
            ),
        ),
        (true, false) => (
            Declarations::FieldMismatch,
            format!(
                "Mismatched field name {}, expected {}.",
                new_field_name, old_field_name
            ),
        ),
        (false, true) => (
            Declarations::TypeMismatch,
            format!(
                "Mismatched field type {}, expected {}.",
                new_field_type, old_field_type
            ),
        ),
        (false, false) => unreachable!("Fields should not be the same"),
    }
}

/// Return a diagnostic for a field mismatch
/// Start by checking the lengths of the fields and return a diagnostic if they are different.
/// If the lengths are the same check each field piece wise and return a diagnostic for each mismatch.
fn struct_field_mismatch_diag(
    struct_name: &Identifier,
    old_struct: &Struct,
    new_struct: &Struct,
    compiled_unit_with_source: &CompiledUnitWithSource,
    lookup: &IdentifierTableLookup,
) -> Result<Diagnostics, Error> {
    let mut diags = Diagnostics::new();

    let struct_index = lookup
        .struct_identifier_to_index
        .get(struct_name)
        .context("Unable to get struct index")?;

    let struct_sourcemap = compiled_unit_with_source
        .unit
        .source_map
        .get_struct_source_map(StructDefinitionIndex::new(*struct_index))
        .with_context(|| format!("Unable to get struct source map {struct_name}"))?;

    let def_loc = struct_sourcemap.definition_location;

    let old_fields: Vec<&Field> = old_struct
        .fields
        .iter()
        .filter(|f| f.name != Identifier::new("dummy_field").unwrap() && f.type_ != Type::Bool)
        .collect();

    let new_fields: Vec<&Field> = new_struct
        .fields
        .iter()
        .filter(|f| f.name != Identifier::new("dummy_field").unwrap() && f.type_ != Type::Bool)
        .collect();

    if old_fields.len() != new_fields.len() {
        diags.add(Diagnostic::new(
            Declarations::TypeMismatch,
            (
                def_loc,
                format!(
                    "Incorrect number of fields: expected {}, found {}",
                    old_fields.len(),
                    new_fields.len()
                ),
            ),
            Vec::<(Loc, String)>::new(),
            vec![
                "Structs are part of a module's public interface and \
                cannot be changed during an upgrade."
                    .to_string(),
                format!(
                    "Restore the original struct's {} \
                    for struct '{struct_name}' including the ordering.",
                    singular_or_plural(old_fields.len(), "field", "fields")
                ),
            ],
        ));
    } else if old_fields != new_fields {
        for (i, (old_field, new_field)) in old_fields.iter().zip(new_fields.iter()).enumerate() {
            if old_field != new_field {
                let field_loc = struct_sourcemap
                    .fields
                    .get(i)
                    .context("Unable to get field location")?;

                let (code, label) = field_mismatch_message(old_field, new_field);

                diags.add(Diagnostic::new(
                    code,
                    (*field_loc, label),
                    vec![(def_loc, "Struct definition".to_string())],
                    vec![
                        "Structs are part of a module's public interface \
                        and cannot be changed during an upgrade."
                            .to_string(),
                        format!(
                            "Restore the original struct's {} for \
                            struct '{struct_name}' including the ordering.",
                            singular_or_plural(old_fields.len(), "field", "fields")
                        ),
                    ],
                ));
            }
        }
    }

    Ok(diags)
}

/// Return a diagnostic for a type parameter mismatch
/// start by checking the lengths of the type parameters and return a diagnostic if they are different
/// if the lengths are the same check each type parameter piece wise and return a diagnostic for each mismatch
fn struct_type_param_mismatch_diag(
    name: &Identifier,
    old_struct: &Struct,
    new_struct: &Struct,
    compiled_unit_with_source: &CompiledUnitWithSource,
    lookup: &IdentifierTableLookup,
) -> Result<Diagnostics, Error> {
    let struct_index = lookup
        .struct_identifier_to_index
        .get(name)
        .context("Unable to get struct index")?;

    let struct_sourcemap = compiled_unit_with_source
        .unit
        .source_map
        .get_struct_source_map(StructDefinitionIndex::new(*struct_index))
        .context("Unable to get struct source map")?;

    let def_loc = struct_sourcemap.definition_location;

    type_parameter_diag(
        "struct",
        name,
        &old_struct.type_parameters,
        &new_struct.type_parameters,
        def_loc,
        &struct_sourcemap.type_parameters,
    )
}

/// Return a diagnostic for a new variant in an enum
fn enum_ability_mismatch_diag(
    enum_name: &Identifier,
    old_enum: &Enum,
    new_enum: &Enum,
    compiled_unit_with_source: &CompiledUnitWithSource,
    lookup: &IdentifierTableLookup,
) -> Result<Diagnostics, Error> {
    let mut diags = Diagnostics::new();

    let enum_index = lookup
        .enum_identifier_to_index
        .get(enum_name)
        .context("Unable to get enum index")?;

    let enum_sourcemap = compiled_unit_with_source
        .unit
        .source_map
        .get_enum_source_map(EnumDefinitionIndex::new(*enum_index))
        .context("Unable to get enum source map")?;

    let def_loc = enum_sourcemap.definition_location;

    if old_enum.abilities != new_enum.abilities {
        let old_abilities: Vec<String> = old_enum
            .abilities
            .into_iter()
            .map(|a| format!("'{:?}'", a).to_lowercase())
            .collect();

        diags.add(Diagnostic::new(
            Declarations::AbilityMismatch,
            (
                def_loc,
                ability_mismatch_label(
                    old_enum.abilities,
                    new_enum.abilities,
                    "ability",
                    "abilities",
                ),
            ),
            Vec::<(Loc, String)>::new(),
            vec![
                "Enums are part of a module's public interface \
                and cannot be changed during an upgrade."
                    .to_string(),
                format!(
                    "Restore the original {} of the enum: {} \
                    for enum '{enum_name}'.",
                    singular_or_plural(old_abilities.len(), "ability", "abilities"),
                    format_list(
                        old_enum
                            .abilities
                            .into_iter()
                            .map(|a| format!("'{:?}'", a).to_lowercase()),
                        None
                    ),
                ),
            ],
        ));
    }
    Ok(diags)
}

/// Returns the error code and label for mismatched, missing, or unexpected variant
fn enum_variant_field_error(
    old_variant: &Variant,
    new_variant: &Variant,
    variant_loc: Loc,
    def_loc: Loc,
) -> (DiagnosticInfo, Vec<String>) {
    if old_variant.fields.len() != new_variant.fields.len() {
        return (
            Declarations::FieldMismatch.into(),
            vec![format!(
                "Mismatched variant field count, expected {}, found {}.",
                old_variant.fields.len(),
                new_variant.fields.len()
            )],
        );
    }

    match (
        old_variant.name != new_variant.name,
        old_variant.fields != new_variant.fields,
    ) {
        (true, true) => (
            Enums::VariantMismatch.into(),
            vec![format!(
                "Mismatched variant '{}', expected '{}'.",
                new_variant.name, old_variant.name
            )],
        ),
        (true, false) => (
            Enums::VariantMismatch.into(),
            vec![format!(
                "Mismatched variant name '{}', expected '{}'.",
                new_variant.name, old_variant.name
            )],
        ),
        (false, true) => {
            let mut errors: Vec<String> = vec![];

            for (i, (old_field, new_field)) in old_variant
                .fields
                .iter()
                .zip(new_variant.fields.iter())
                .enumerate()
            {
                if old_field != new_field {
                    errors.push(format!(
                        "Mismatched field {}, expected {}.",
                        field_to_string(old_field).0,
                        field_to_string(new_field).2
                    ));
                }
            }

            (Declarations::FieldMismatch.into(), errors)
        }
        (false, false) => unreachable!("Variants should not be the same"),
    }
}

/// Return a diagnostic for a type parameter mismatch
/// start by checking the lengths of the type parameters and return a diagnostic if they are different
/// if the lengths are the same check each type parameter piece wise and return a diagnostic for each mismatch
fn enum_variant_mismatch_diag(
    enum_name: &Identifier,
    old_enum: &Enum,
    new_enum: &Enum,
    compiled_unit_with_source: &CompiledUnitWithSource,
    lookup: &IdentifierTableLookup,
) -> Result<Diagnostics, Error> {
    let mut diags = Diagnostics::new();

    let enum_index = lookup
        .enum_identifier_to_index
        .get(enum_name)
        .context("Unable to get enum index")?;

    let enum_sourcemap = compiled_unit_with_source
        .unit
        .source_map
        .get_enum_source_map(EnumDefinitionIndex::new(*enum_index))
        .context("Unable to get enum source map")?;

    let def_loc = enum_sourcemap.definition_location;

    for (i, (old_variant, new_variant)) in old_enum
        .variants
        .iter()
        .zip(new_enum.variants.iter())
        .enumerate()
    {
        if old_variant != new_variant {
            let variant_loc = enum_sourcemap
                .variants
                .get(i)
                .context("Unable to get variant location")?
                .0
                 .1;

            let (code, labels) =
                enum_variant_field_error(old_variant, new_variant, variant_loc, def_loc);

            for label in labels {
                diags.add(Diagnostic::new(
                    code.clone(),
                    (variant_loc, label),
                    vec![(def_loc, "Enum definition".to_string())],
                    vec![
                        "Enums are part of a module's public interface \
                    and cannot be changed during an upgrade."
                            .to_string(),
                        format!(
                            "Restore the original enum's {} for \
                        enum '{enum_name}' including the ordering.",
                            singular_or_plural(old_enum.variants.len(), "variant", "variants")
                        ),
                    ],
                ));
            }
        }
    }

    Ok(diags)
}

/// Return a diagnostic for a new variant in an enum
fn enum_new_variant_diag(
    enum_name: &Identifier,
    old_enum: &Enum,
    new_enum: &Enum,
    compiled_unit_with_source: &CompiledUnitWithSource,
    lookup: &IdentifierTableLookup,
) -> Result<Diagnostics, Error> {
    let mut diags = Diagnostics::new();

    let enum_index = lookup
        .enum_identifier_to_index
        .get(enum_name)
        .context("Unable to get enum index")?;

    let enum_sourcemap = compiled_unit_with_source
        .unit
        .source_map
        .get_enum_source_map(EnumDefinitionIndex::new(*enum_index))
        .context("Unable to get enum source map")?;

    let old_enum_map = old_enum
        .variants
        .iter()
        .map(|v| &v.name)
        .collect::<HashSet<_>>();

    let def_loc = enum_sourcemap.definition_location;

    for (i, new_variant) in new_enum.variants.iter().enumerate() {
        if !old_enum_map.contains(&new_variant.name) {
            let variant_loc = enum_sourcemap
                .variants
                .get(i)
                .context("Unable to get variant location")?
                .0
                 .1;

            diags.add(Diagnostic::new(
                Enums::VariantMismatch,
                (
                    variant_loc,
                    format!("New unexpected variant '{}'.", new_variant.name),
                ),
                vec![(def_loc, "Enum definition".to_string())],
                vec![
                    "Enums are part of a module's public interface and cannot be \
                    changed during an upgrade."
                        .to_string(),
                    format!(
                        "Restore the original enum's {} for enum \
                        '{enum_name}' including the ordering.",
                        singular_or_plural(old_enum.variants.len(), "variant", "variants")
                    ),
                ],
            ))
        }
    }

    Ok(diags)
}

/// Return a diagnostic for a missing variant in an enum
fn enum_variant_missing_diag(
    enum_name: &Identifier,
    old_enum: &Enum,
    tag: usize,
    compiled_unit_with_source: &CompiledUnitWithSource,
    lookup: &IdentifierTableLookup,
) -> Result<Diagnostics, Error> {
    let mut diags = Diagnostics::new();

    let enum_index = lookup
        .enum_identifier_to_index
        .get(enum_name)
        .context("Unable to get enum index")?;

    let enum_sourcemap = compiled_unit_with_source
        .unit
        .source_map
        .get_enum_source_map(EnumDefinitionIndex::new(*enum_index))
        .context("Unable to get enum source map")?;

    let variant_name = &old_enum
        .variants
        .get(tag)
        .context("Unable to get variant")?
        .name;

    diags.add(Diagnostic::new(
        Enums::VariantMismatch,
        (
            enum_sourcemap.definition_location,
            format!("Missing variant '{variant_name}'.",),
        ),
        Vec::<(Loc, String)>::new(),
        vec![
            "Enums are part of a module's public interface and cannot \
            be changed during an upgrade."
                .to_string(),
            format!(
                "Restore the original enum's variant '{variant_name}' for enum \
                '{enum_name}' including the ordering."
            ),
        ],
    ));

    Ok(diags)
}

/// Return a diagnostic for a type parameter mismatch
fn enum_type_param_mismatch(
    enum_name: &Identifier,
    old_enum: &Enum,
    new_enum: &Enum,
    compiled_unit_with_source: &CompiledUnitWithSource,
    lookup: &IdentifierTableLookup,
) -> Result<Diagnostics, Error> {
    let enum_index = lookup
        .enum_identifier_to_index
        .get(enum_name)
        .context("Unable to get enum index")?;

    let enum_sourcemap = compiled_unit_with_source
        .unit
        .source_map
        .get_enum_source_map(EnumDefinitionIndex::new(*enum_index))
        .context("Unable to get enum source map")?;

    let def_loc = enum_sourcemap.definition_location;

    type_parameter_diag(
        "enum",
        enum_name,
        &old_enum.type_parameters,
        &new_enum.type_parameters,
        def_loc,
        &enum_sourcemap.type_parameters,
    )
}

/// Return a diagnostic for a type parameter mismatch
fn type_parameter_diag(
    declaration_kind: &str,
    name: &Identifier,
    old_type_parameters: &[DatatypeTyParameter],
    new_type_parameters: &[DatatypeTyParameter],
    def_loc: Loc,
    type_parameter_locs: &[SourceName],
) -> Result<Diagnostics, Error> {
    let mut diags = Diagnostics::new();

    // capitalize the first letter
    let capital_declaration_kind = declaration_kind
        .chars()
        .enumerate()
        .map(|(i, c)| {
            if i == 0 {
                c.to_uppercase().next().unwrap()
            } else {
                c
            }
        })
        .collect::<String>();

    if old_type_parameters.len() != new_type_parameters.len() {
        diags.add(Diagnostic::new(
            Declarations::TypeParamMismatch,
            (
                def_loc,
                format!(
                    "Incorrect number of type parameters: expected {}, found {}",
                    old_type_parameters.len(),
                    new_type_parameters.len()
                ),
            ),
            Vec::<(Loc, String)>::new(),
            vec![
                format!(
                    "{capital_declaration_kind}s are part of a module's public interface and \
                cannot be changed during an upgrade."
                ),
                format!(
                    "Restore the original {declaration_kind}'s type {} \
                    for {declaration_kind} '{name}' including the ordering.",
                    singular_or_plural(old_type_parameters.len(), "parameter", "parameters"),
                ),
            ],
        ));
    } else if old_type_parameters != new_type_parameters {
        for (i, (old_type_param, new_type_param)) in old_type_parameters
            .iter()
            .zip(new_type_parameters.iter())
            .enumerate()
        {
            let type_param_loc = type_parameter_locs
                .get(i)
                .context("Unable to get type parameter location")?;

            if let Some((label, fix_note)) =
                type_param_constraint_labels(old_type_param.constraints, new_type_param.constraints)
            {
                diags.add(Diagnostic::new(
                    Declarations::TypeParamMismatch,
                    (type_param_loc.1, label),
                    vec![(def_loc, format!("{capital_declaration_kind} definition"))],
                    vec![
                        format!(
                            "{capital_declaration_kind}s are part of a module's public interface \
                        and cannot be changed during an upgrade."
                        ),
                        fix_note,
                    ],
                ));
            }

            if let Some((label, fix_note)) =
                type_param_phantom_labels(old_type_param.is_phantom, new_type_param.is_phantom)
            {
                diags.add(Diagnostic::new(
                    Declarations::TypeParamMismatch,
                    (type_param_loc.1, label),
                    vec![(def_loc, format!("{capital_declaration_kind} definition"))],
                    vec![
                        format!(
                            "{capital_declaration_kind}s are part of a module's public interface \
                        and cannot be changed during an upgrade."
                        ),
                        fix_note,
                    ],
                ));
            }
        }
    }
    Ok(diags)
}

/// Return a diagnostic for a type parameter constrant mismatch
fn type_param_constraint_labels(
    old_constraints: AbilitySet,
    new_constraints: AbilitySet,
) -> Option<(String, String)> {
    if old_constraints == new_constraints {
        return None;
    }

    let old_abilities_list: Vec<String> = old_constraints
        .into_iter()
        .map(|a| format!("'{a}'").to_lowercase())
        .collect();

    Some((
        ability_mismatch_label(
            old_constraints,
            new_constraints,
            "constraint",
            "constraints",
        ),
        format!(
            "Restore the original type parameter {}",
            format_list(old_abilities_list, Some(("constraint", "constraints"))),
        ),
    ))
}

/// Return a diagnostic for a type parameter phantom mismatch
fn type_param_phantom_labels(old_phantom: bool, new_phantom: bool) -> Option<(String, String)> {
    if old_phantom == new_phantom {
        return None;
    }

    Some(if old_phantom {
        (
            "Missing 'phantom' modifier".to_string(),
            "Restore the original 'phantom' modifier".to_string(),
        )
    } else {
        (
            "Unexpected 'phantom' modifier".to_string(),
            "Remove the 'phantom' modifier".to_string(),
        )
    })
}

/// Format a list of items into a human-readable string.
fn format_list(
    items: impl IntoIterator<Item = impl std::fmt::Display>,
    noun_singular_plural: Option<(&str, &str)>,
) -> String {
    let items: Vec<_> = items.into_iter().map(|i| i.to_string()).collect();
    let items_string = match items.len() {
        0 => "none".to_string(),
        1 => items[0].to_string(),
        2 => format!("{} and {}", items[0], items[1]),
        _ => {
            let all_but_last = &items[..items.len() - 1].join(", ");
            let last = items.last().unwrap();
            format!("{}, and {}", all_but_last, last)
        }
    };
    if let Some((singular, plural)) = noun_singular_plural {
        format!(
            "{}: {}",
            singular_or_plural(items.len(), singular, plural),
            items_string,
        )
    } else {
        items_string
    }
}

/// Return a string with the singular or plural form of a word.
fn singular_or_plural(n: usize, singular: &str, plural: &str) -> String {
    if n == 1 {
        singular.to_string()
    } else {
        plural.to_string()
    }
}

/// Helper function to determine if colors should be used in the output.
/// disables colors in tests
fn use_colors() -> bool {
    #[cfg(test)]
    {
        false
    }

    #[cfg(not(test))]
    {
        use std::io::{stdout, IsTerminal};
        stdout().is_terminal()
    }
}
