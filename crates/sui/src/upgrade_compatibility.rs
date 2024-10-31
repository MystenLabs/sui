// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[path = "unit_tests/upgrade_compatibility_tests.rs"]
#[cfg(test)]
mod upgrade_compatibility_tests;

use codespan_reporting::diagnostic::Label;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::{stdout, IsTerminal};
use std::sync::Arc;

use anyhow::{anyhow, Context, Error};

use move_binary_format::file_format::{
    AbilitySet, EnumDefinitionIndex, FunctionDefinitionIndex, StructDefinitionIndex, TableIndex,
};
use move_binary_format::{
    compatibility::Compatibility,
    compatibility_mode::CompatibilityMode,
    file_format::Visibility,
    normalized::{Enum, Function, Module, Struct},
    CompiledModule,
};
use move_command_line_common::files::FileHash;
use move_compiler::diagnostics::codes::DiagnosticInfo;
use move_compiler::{
    diag,
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
use move_ir_types::location::Loc;
use move_package::compilation::compiled_package::CompiledUnitWithSource;
use sui_json_rpc_types::{SuiObjectDataOptions, SuiRawData};
use sui_move_build::CompiledPackage;
use sui_protocol_config::ProtocolConfig;
use sui_sdk::SuiClient;
use sui_types::{base_types::ObjectID, execution_config_utils::to_binary_config};

/// Errors that can occur during upgrade compatibility checks.
/// one-to-one related to the underlying trait functions see: [`CompatibilityMode`]
#[derive(Debug, Clone)]
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

            // impl into diagnostic info
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
upgrade_codes!(
    Declarations: [
        PublicMissing: { msg: "missing public declaration" },
    ],
);

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

    compare_packages(existing_modules, new_package)
}

/// Collect all the errors into a single error message.
fn compare_packages(
    existing_modules: Vec<CompiledModule>,
    new_package: CompiledPackage,
) -> Result<(), Error> {
    // create a map from the new modules
    let new_modules_map: HashMap<Identifier, CompiledModule> = new_package
        .get_modules()
        .map(|m| (m.self_id().name().to_owned(), m.clone()))
        .collect();

    let lookup: HashMap<Identifier, IdentifierTableLookup> = existing_modules
        .iter()
        .map(|m| (m.self_id().name().to_owned(), table_index(m)))
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

    let mut files: FilesSourceText = HashMap::new();
    let mut file_set = HashSet::new();

    let mut diags = Diagnostics::new();

    for (name, err) in errors {
        let compiled_unit_with_source = new_package
            .package
            .get_module_by_name_from_root(name.as_str())
            .context("Unable to get module")?;

        if !file_set.contains(&compiled_unit_with_source.source_path) {
            let file_contents: Arc<str> =
                fs::read_to_string(&compiled_unit_with_source.source_path)
                    .context("Unable to read source file")?
                    .into();
            let file_hash = FileHash::new(&file_contents);

            files.insert(
                file_hash,
                (
                    FileName::from(compiled_unit_with_source.source_path.to_string_lossy()),
                    file_contents,
                ),
            );

            file_set.insert(compiled_unit_with_source.source_path.clone());
        }

        diags.extend(diag_from_error(
            &err,
            compiled_unit_with_source,
            &lookup[&name],
        )?);
    }

    Err(anyhow!(
        "{}\nUpgrade failed, this package requires changes to be compatible with the existing package. Its upgrade policy is set to 'Compatible'.",
        String::from_utf8(report_diagnostics_to_buffer(&files.into(), diags, stdout().is_terminal())).context("Unable to convert buffer to string")?
    ))
}

/// Convert an error to a diagnostic using the specific error type's function.
fn diag_from_error(
    error: &UpgradeCompatibilityModeError,
    compiled_unit_with_source: &CompiledUnitWithSource,
    lookup: &IdentifierTableLookup,
) -> Result<Diagnostics, Error> {
    match error {
        UpgradeCompatibilityModeError::StructMissing { name, .. } => missing_definition_diag(
            Declarations::PublicMissing,
            "struct",
            &name,
            compiled_unit_with_source,
        ),
        UpgradeCompatibilityModeError::EnumMissing { name, .. } => missing_definition_diag(
            Declarations::PublicMissing,
            "enum",
            &name,
            compiled_unit_with_source,
        ),
        UpgradeCompatibilityModeError::StructMissing { name, .. } => missing_definition_diag(
            Declarations::PublicMissing,
            "struct",
            &name,
            compiled_unit_with_source,
        ),

        UpgradeCompatibilityModeError::StructAbilityMismatch {
            name,
            old_struct,
            new_struct,
        } => struct_ability_mismatch_diag(
            Declarations::PublicMissing,
            &name,
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
            Declarations::PublicMissing,
            &name,
            old_struct,
            new_struct,
            compiled_unit_with_source,
            lookup,
        ),
        UpgradeCompatibilityModeError::EnumMissing { name, .. } => missing_definition_diag(
            Declarations::PublicMissing,
            "enum",
            &name,
            compiled_unit_with_source,
        ),
        UpgradeCompatibilityModeError::EnumAbilityMismatch {
            name,
            old_enum,
            new_enum,
        } => enum_ability_mismatch_diag(
            Declarations::PublicMissing,
            &name,
            old_enum,
            new_enum,
            compiled_unit_with_source,
            lookup,
        ),

        UpgradeCompatibilityModeError::EnumNewVariant {
            name,
            old_enum,
            new_enum,
        } => enum_new_variant_diag(
            Declarations::PublicMissing,
            &name,
            old_enum,
            new_enum,
            // *tag,
            compiled_unit_with_source,
            lookup,
        ),

        UpgradeCompatibilityModeError::EnumVariantMissing { name, tag, .. } => {
            enum_variant_missing_diag(
                Declarations::PublicMissing, // TODO
                &name,
                *tag,
                compiled_unit_with_source,
                lookup,
            )
        }

        UpgradeCompatibilityModeError::EnumVariantMismatch {
            name,
            old_enum,
            new_enum,
            ..
        } => enum_variant_mismatch_diag(
            Declarations::PublicMissing, // TODO
            &name,
            old_enum,
            new_enum,
            compiled_unit_with_source,
            lookup,
        ),

        UpgradeCompatibilityModeError::FunctionMissingPublic { name, .. } => {
            missing_definition_diag(
                Declarations::PublicMissing, // TODO
                "public function",
                &name,
                compiled_unit_with_source,
            )
        }
        UpgradeCompatibilityModeError::FunctionMissingEntry { name, .. } => {
            missing_definition_diag(
                Declarations::PublicMissing, // TODO
                "entry function",
                &name,
                compiled_unit_with_source,
            )
        }
        UpgradeCompatibilityModeError::FunctionSignatureMismatch {
            name,
            old_function,
            new_function,
        } => function_signature_mismatch_diag(
            Declarations::PublicMissing, // TODO
            &name,
            old_function,
            new_function,
            compiled_unit_with_source,
            lookup,
        ),
        _ => todo!("Implement diag_from_error for {:?}", error),
    }
}

/// Return a diagnostic for a missing definition.
fn missing_definition_diag(
    error: impl Into<DiagnosticInfo> + Copy,
    declaration_kind: &str,
    identifier_name: &Identifier,
    compiled_unit_with_source: &CompiledUnitWithSource,
) -> Result<Diagnostics, Error> {
    let module_name = compiled_unit_with_source.unit.name;
    let loc = compiled_unit_with_source
        .unit
        .source_map
        .definition_location;

    let mut diags = Diagnostics::new();

    diags.add(
        Diagnostic::new(
            error,
            (loc, format!(
                "{declaration_kind} '{identifier_name}' is missing",
                declaration_kind = declaration_kind,
                identifier_name = identifier_name,
            )),
            std::iter::empty::<(Loc, String)>(),
            vec![format!(
                "{declaration_kind} is missing expected {declaration_kind} '{identifier_name}', but found none",
            ),
                 format!(
                     "{declaration_kind}s are part of a module's public interface and cannot be removed or changed during an upgrade",
                 ),
                 format!(
                     "add missing {declaration_kind} '{identifier_name}' back to the module '{module_name}'.",
                 )]
        )
    );

    Ok(diags)
}

/// Return a diagnostic for a function signature mismatch.
/// start by checking the lengths of the parameters and returns and return a diagnostic if they are different
/// if the lengths are the same check each parameter piece wise and return a diagnostic for each mismatch
fn function_signature_mismatch_diag(
    error: impl Into<DiagnosticInfo> + std::marker::Copy,
    function_name: &Identifier,
    old_function: &Function,
    new_function: &Function,
    compiled_unit_with_source: &CompiledUnitWithSource,
    lookup: &IdentifierTableLookup,
) -> Result<Diagnostics, Error> {
    let module_name = compiled_unit_with_source.unit.name;
    let old_func_index = lookup
        .function_identifier_to_index
        .get(function_name)
        .context("Unable to get function index")?;

    let new_func_sourcemap = compiled_unit_with_source
        .unit
        .source_map
        .get_function_source_map(FunctionDefinitionIndex::new(*old_func_index))
        .context("Unable to get function source map")?;

    let identifier_loc = new_func_sourcemap.definition_location;

    let mut diags = Diagnostics::new();

    // handle function arguments
    if old_function.parameters.len() != new_function.parameters.len() {
        diags.add(
            Diagnostic::new(
                Declarations::PublicMissing, // TODO
                (identifier_loc, format!("Function '{function_name}' expected {} parameters, have {}", old_function.parameters.len(), new_function.parameters.len())),
                Vec::<(Loc, String)>::new(),
                vec![format!("Functions are part of a module's public interface and cannot be changed during an upgrade, restore the original function's parameters for function '{function_name}', expected {} parameters.", old_function.parameters.len())],
            )
        );
    } else if old_function.parameters.eq(&new_function.parameters) {
        for ((i, old_param), new_param) in old_function
            .parameters
            .iter()
            .enumerate()
            .zip(new_function.parameters.iter())
        {
            if old_param != new_param {
                let param_loc = new_func_sourcemap
                    .parameters
                    .get(i)
                    .context("Unable to get parameter location")?
                    .1;

                diags.add(
                    Diagnostic::new(
                        Declarations::PublicMissing,
                        (param_loc, format!("Function '{function_name}' unexpected parameter {new_param} at position {i}, expected {old_param}")),
                        Vec::<(Loc, String)>::new(),
                        vec![format!("Functions are part of a module's public interface and cannot be changed during an upgrade, restore the original function's parameters for function '{function_name}'.")],
                    )
                );
            }
        }
    }

    // handle return
    if old_function.return_.len() != new_function.return_.len() {
        diags.add(
            Diagnostic::new(
                Declarations::PublicMissing, // TODO
                (identifier_loc, format!("Function '{function_name}' expected to have {} return type(s), have {}", old_function.return_.len(), new_function.return_.len())),
                Vec::<(Loc, String)>::new(),
                vec![format!("Functions are part of a module's public interface and cannot be changed during an upgrade, restore the original function's return types for function '{function_name}'.")],
            )
        );
    } else if old_function.return_ != new_function.return_ {
        for ((i, old_return), new_return) in old_function
            .return_
            .iter()
            .enumerate()
            .zip(new_function.return_.iter())
        {
            let return_ = new_func_sourcemap
                .returns
                .get(i)
                .context("Unable to get return location")?;

            if old_return != new_return {
                diags.add(
                    Diagnostic::new(
                        Declarations::PublicMissing,
                        (*return_, if new_function.return_.len() == 1 {
                            format!("Function '{function_name}' has an unexpected return type {new_return}, expected {old_return}")
                        } else {
                            format!("Function '{function_name}' unexpected return type {new_return} at position {i}, expected {old_return}")
                        }),
                        Vec::<(Loc, String)>::new(),
                        vec!["Functions are part of a module's public interface and cannot be changed during an upgrade".to_string(), format!("restore the original function's return types for function '{function_name}'.")],
                    )
                );
            }
        }
    }

    Ok(diags)
}

fn struct_ability_mismatch_diag(
    error_code: impl Into<DiagnosticInfo> + Copy,
    struct_name: &Identifier,
    old_struct: &Struct,
    new_struct: &Struct,
    compiled_unit_with_source: &CompiledUnitWithSource,
    lookup: &IdentifierTableLookup,
) -> Result<Diagnostics, Error> {
    let old_struct_index = lookup
        .struct_identifier_to_index
        .get(struct_name)
        .context("Unable to get struct index")?;
    let struct_sourcemap = compiled_unit_with_source
        .unit
        .source_map
        .get_struct_source_map(StructDefinitionIndex::new(*old_struct_index))
        .context("Unable to get struct source map")?;

    let mut diags = Diagnostics::new();

    if old_struct.abilities != new_struct.abilities {
        let def_loc = struct_sourcemap.definition_location;

        let missing_abilities =
            AbilitySet::from_u8(old_struct.abilities.into_u8() & !new_struct.abilities.into_u8())
                .context("Unable to get missing abilities")?;
        let extra_abilities =
            AbilitySet::from_u8(new_struct.abilities.into_u8() & !old_struct.abilities.into_u8())
                .context("Unable to get extra abilities")?;

        let label = match (
            missing_abilities != AbilitySet::EMPTY,
            extra_abilities != AbilitySet::EMPTY,
        ) {
            (true, true) => format!(
                "Struct '{struct_name}' has unexpected abilities, missing {:?}, unexpected {:?}",
                missing_abilities, extra_abilities
            ),
            (true, false) => format!(
                "Struct '{struct_name}' has missing abilities {:?}",
                missing_abilities
            ),
            (false, true) => format!(
                "Struct '{struct_name}' has unexpected abilities {:?}",
                extra_abilities
            ),
            (false, false) => unreachable!("Abilities should not be the same"),
        };

        diags.add(
            Diagnostic::new(
                error_code,
                (def_loc, "Struct ability mismatch"),
                Vec::<(Loc, String)>::new(),
                vec![format!(
                    "Structs are part of a module's public interface and cannot be changed during an upgrade, restore the original struct's abilities for struct '{struct_name}'."
                )],
            )
        );
    }

    Ok(diags)
}

fn struct_field_mismatch_diag(
    error_code: impl Into<DiagnosticInfo> + Copy,
    struct_name: &Identifier,
    old_struct: &Struct,
    new_struct: &Struct,
    compiled_unit_with_source: &CompiledUnitWithSource,
    lookup: &IdentifierTableLookup,
) -> Result<Diagnostics, Error> {
    let old_struct_index = lookup
        .struct_identifier_to_index
        .get(struct_name)
        .context("Unable to get struct index")?;
    let struct_sourcemap = compiled_unit_with_source
        .unit
        .source_map
        .get_struct_source_map(StructDefinitionIndex::new(*old_struct_index))
        .context("Unable to get struct source map")?;

    let mut diags = Diagnostics::new();

    if old_struct.fields.len() != new_struct.fields.len() {
        let def_loc = struct_sourcemap.definition_location;

        diags.add(Diagnostic::new(
            error_code,
            (def_loc, format!("Struct '{struct_name}' has a different number of fields, expected {}, found {}", old_struct.fields.len(), new_struct.fields.len())),
            Vec::<(Loc, String)>::new(),
            vec![format!(
                // TODO
                "Structs are part of a module's public interface and cannot be changed during an upgrade, restore the original struct's fields for struct '{struct_name}'."
            )],
        ));
    } else if old_struct.fields != new_struct.fields {
        for (i, (old_field, new_field)) in old_struct
            .fields
            .iter()
            .zip(new_struct.fields.iter())
            .enumerate()
        {
            if old_field != new_field {
                let field_loc = struct_sourcemap
                    .fields
                    .get(i)
                    .context("Unable to get field location")?
                    .clone();

                // match of the above
                // TODO ?
                let label = match (old_field.name != new_field.name, old_field.type_ != new_field.type_) {
                    (true, true) => format!(
                        "Struct '{struct_name}' has different fields `{}: {}` at position {i}, expected `{}: {}`.",
                        new_field.name, new_field.type_, old_field.name, old_field.type_
                    ),
                    (true, false) => format!(
                        "Struct '{struct_name}' has different field names '{}' at position {i}, expected '{}'.",
                        new_field.name, old_field.name
                    ),
                    (false, true) => format!(
                        "Struct '{struct_name}' has different field types '{}' at position {i}, expected '{}'.",
                        new_field.type_, old_field.type_
                    ),
                    (false, false) => unreachable!("Fields should no be the same"),
                };

                diags.add(
                    Diagnostic::new(
                        error_code,
                        (field_loc, format!(
                            "Struct '{struct_name}' has different fields `{}: {}` at position {i}, expected `{}: {}`.",
                            new_field.name, new_field.type_, old_field.name, old_field.type_
                        )),
                        Vec::<(Loc, String)>::new(),
                        vec![format!(
                            "Structs are part of a module's public interface and cannot be changed during an upgrade, restore the original struct's fields for struct '{struct_name}' including the ordering."
                        )],
                    )
                );

                diags.add(Diagnostic::new(
                    error_code,
                    (field_loc, format!(
                        "Struct '{struct_name}' has different fields `{}: {}` at position {i}, expected `{}: {}`.",
                        new_field.name, new_field.type_, old_field.name, old_field.type_
                    )),
                    Vec::<(Loc, String)>::new(),
                    vec![format!(
                        "Structs are part of a module's public interface and cannot be changed during an upgrade, restore the original struct's fields for struct '{struct_name}' including the ordering."
                    )],
                ))
            }
        }
    }

    Ok(diags)
}

fn enum_ability_mismatch_diag(
    error_code: impl Into<DiagnosticInfo> + Copy,
    enum_name: &Identifier,
    old_enum: &Enum,
    new_enum: &Enum,
    compiled_unit_with_source: &CompiledUnitWithSource,
    lookup: &IdentifierTableLookup,
) -> Result<Diagnostics, Error> {
    let mut diags = Diagnostics::new();

    let old_enum_index = lookup
        .enum_identifier_to_index
        .get(enum_name)
        .context("Unable to get enum index")?;

    let enum_sourcemap = compiled_unit_with_source
        .unit
        .source_map
        .get_enum_source_map(EnumDefinitionIndex::new(*old_enum_index))
        .context("Unable to get enum source map")?;

    let def_loc = enum_sourcemap.definition_location;

    if old_enum.abilities != new_enum.abilities {
        let missing_abilities =
            AbilitySet::from_u8(old_enum.abilities.into_u8() & !new_enum.abilities.into_u8())
                .context("Unable to get missing abilities")?;
        let extra_abilities =
            AbilitySet::from_u8(new_enum.abilities.into_u8() & !old_enum.abilities.into_u8())
                .context("Unable to get extra abilities")?;

        let label = match (
            missing_abilities != AbilitySet::EMPTY,
            extra_abilities != AbilitySet::EMPTY,
        ) {
            (true, true) => format!(
                "Enum '{enum_name}' has unexpected abilities, missing {:?}, unexpected {:?}",
                missing_abilities, extra_abilities
            ),
            (true, false) => format!(
                "Enum '{enum_name}' has missing abilities {:?}",
                missing_abilities
            ),
            (false, true) => format!(
                "Enum '{enum_name}' has unexpected abilities {:?}",
                extra_abilities
            ),
            (false, false) => unreachable!("Abilities should not be the same"),
        };

        diags.add( // TODO CHECK
            //        diag!(
            //     error_code,
            //     (def_loc, "Enum ability mismatch"),
            //     // TODO secondary enum
            //     note: format!(
            //         "Enums are part of a module's public interface and cannot be changed during an upgrade, restore the original enum's abilities for enum '{enum_name}'.",
            //     ),
            // ),
            Diagnostic::new(
                error_code,
                (def_loc, format!(
                    "Enum '{enum_name}' has unexpected abilities, missing {:?}, unexpected {:?}",
                    missing_abilities, extra_abilities
                )),
                Vec::<(Loc, String)>::new(),
                vec![format!( // TODO breakup
                    "Enums are part of a module's public interface and cannot be changed during an upgrade, restore the original enum's abilities for enum '{enum_name}'."
                )],
            )
        );
    }
    Ok(diags)
}

fn enum_variant_mismatch_diag(
    error: impl Into<DiagnosticInfo> + Copy,
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

    for (i, (old_variant, new_variant)) in old_enum
        .variants
        .iter()
        .zip(new_enum.variants.iter())
        .enumerate()
    {
        if old_variant != new_variant {
            let variant = &enum_sourcemap
                .variants
                .get(i)
                .context("Unable to get variant location")?
                .0;

            let variant_loc = enum_sourcemap.definition_location;

            let label = match (old_variant.name != new_variant.name, old_variant.fields != new_variant.fields) {
                (true, true) => format!(
                    "Enum '{enum_name}' has different variant '{}' at position {i}, expected '{}'.",
                    new_variant.name, old_variant.name
                ),
                (true, false) => format!(
                    "Enum '{enum_name}' has different variant name '{}' at position {i}, expected '{}'.",
                    new_variant.name, old_variant.name
                ),
                (false, true) => {
                    let new_variant_fields = new_variant.fields
                        .iter()
                        .map(|f| format!("{:?}", f))
                        .collect::<Vec<_>>()
                        .join(", ");

                    let old_variant_fields = old_variant.fields
                        .iter()
                        .map(|f| format!("{:?}", f))
                        .collect::<Vec<_>>()
                        .join(", ");

                    format!(
                        "Enum '{enum_name}' has different variant fields '{}' at position {i}, expected '{}'.",
                        new_variant_fields, old_variant_fields)
                }
                (false, false) => unreachable!("Variants should not be the same"),
            };

            diags.add(
                Diagnostic::new(
                    error,
                    (variant_loc, label),
                    Vec::<(Loc, String)>::new(),
                    vec![format!( // TODO breakup
                        "Enums are part of a module's public interface and cannot be changed during an upgrade, restore the original enum's variants for enum '{enum_name}'.",
                    )],
                )
            );
        }
    }

    Ok(diags)
}

fn enum_new_variant_diag(
    error: impl Into<DiagnosticInfo> + Copy,
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
        .map(|v| v.name.clone())
        .collect::<HashSet<_>>();

    let def_loc = enum_sourcemap.definition_location;

    for (i, new_variant) in new_enum.variants.iter().enumerate() {
        if !old_enum_map.contains(&new_variant.name) {
            let variant = &enum_sourcemap
                .variants
                .get(i)
                .context("Unable to get variant location")?
                .0;

            diags.add(
                Diagnostic::new(
                    error,
                    (variant.1, format!(
                        "Enum '{enum_name}' has a new unexpected variant '{}' at position {i}.",
                        new_variant.name
                    )),
                    Vec::<(Loc, String)>::new(),
                    // TODO NEED TO SHOW THE ENUM LINE HERE
                    vec![format!( // TODO breakup
                        "Enums are part of a module's public interface and cannot be changed during an upgrade, restore the original enum's variants for enum '{enum_name}'.",
                    )],
                )
            )
        }
    }

    Ok(diags)
}

fn enum_variant_missing_diag(
    error: impl Into<DiagnosticInfo> + Copy,
    enum_name: &Identifier,
    tag: usize,
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

    // let start_def = enum_sourcemap.definition_location.start() as usize;
    // let end_def = enum_sourcemap.definition_location.end() as usize;
    // let enum_label = Label::secondary(file_id, start_def..end_def);

    let mut diags = Diagnostics::new();

    diags.add(diag!(
        error,
        (
            enum_sourcemap.definition_location,
            format!("Enum '{enum_name}' has a missing variant at position {tag}.",)
        ),
    ));

    Ok(diags)
}
