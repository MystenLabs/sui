// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fmt::Display};

use move_binary_format::{CompiledModule, errors::VMError};
use move_core_types::{account_address::AccountAddress, identifier::Identifier};

pub struct Data {
    pub package_data: BTreeMap<AccountAddress, PackageData>,
}

#[derive(Default)]
pub struct PackageData {
    pub modules: BTreeMap<Identifier, ModuleData>,
    // accurate only if all modules are verified successfully
    pub ticks: u128,
}

pub struct ModuleData {
    pub name: Identifier,
    pub module: CompiledModule,
    pub result: ModuleVerificationResult,
}

pub struct ModuleVerificationResult {
    pub status: ModuleVerificationStatus,
    pub ticks: u128,
    pub time: u128, // in microseconds
    pub function_ticks: BTreeMap<String, (u128 /* ticks */, u128 /* microseconds */)>,
}

pub enum ModuleVerificationStatus {
    Verified,
    Failed(VMError),
    FunctionsFailed(BTreeMap<String, VMError>),
}

pub struct DataDisplay<'a> {
    pub data: &'a Data,
    pub show_ticks: bool,
}

impl Data {
    pub fn display(&self, show_ticks: bool) -> DataDisplay<'_> {
        DataDisplay {
            data: self,
            show_ticks,
        }
    }
}

impl Display for DataDisplay<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let show_ticks = self.show_ticks;
        let empty_function_failures = BTreeMap::new();
        for (address, package_data) in &self.data.package_data {
            let PackageData { modules, ticks } = package_data;
            let has_failed = modules
                .values()
                .any(|m| !matches!(m.result.status, ModuleVerificationStatus::Verified));
            let ticks_msg = if show_ticks {
                format!(" ({ticks} ticks)")
            } else {
                String::new()
            };
            if has_failed || show_ticks {
                writeln!(f, "{address}{ticks_msg}")?;
            } else {
                continue;
            }
            for (module_name, module_data) in modules {
                let ticks_msg = if show_ticks {
                    format!(
                        " ({} ticks, {} μs)",
                        module_data.result.ticks, module_data.result.time
                    )
                } else {
                    String::new()
                };
                let status_msg = match &module_data.result.status {
                    ModuleVerificationStatus::Verified => "Verified",
                    ModuleVerificationStatus::Failed(_) => "FAILED",
                    ModuleVerificationStatus::FunctionsFailed(_) => "Functions Failed",
                };
                let has_failed = !matches!(
                    module_data.result.status,
                    ModuleVerificationStatus::Verified
                );
                if has_failed || show_ticks {
                    writeln!(f, "  {module_name}{ticks_msg}: {status_msg}")?;
                } else {
                    continue;
                }
                let function_failures = match &module_data.result.status {
                    ModuleVerificationStatus::FunctionsFailed(failures) => failures,
                    _ => &empty_function_failures,
                };
                for (fname, (ticks, time)) in &module_data.result.function_ticks {
                    if show_ticks {
                        let status = match function_failures.get(fname) {
                            Some(err) => format!("FAILED {}", err),
                            None => "Verified".to_string(),
                        };
                        writeln!(f, "    {fname} ({} ticks, {} μs) {status}", ticks, time)?;
                    } else if let Some(err) = function_failures.get(fname) {
                        writeln!(f, "    {fname}: {}", err)?;
                    }
                }
            }
        }
        let num_packages = self.data.package_data.len();
        let packages_verified = self
            .data
            .package_data
            .values()
            .filter(|p| {
                p.modules
                    .values()
                    .all(|m| matches!(m.result.status, ModuleVerificationStatus::Verified))
            })
            .count();
        let num_modules = self
            .data
            .package_data
            .values()
            .flat_map(|p| p.modules.values())
            .count();
        let modules_verified = self
            .data
            .package_data
            .values()
            .flat_map(|p| p.modules.values())
            .filter(|m| matches!(m.result.status, ModuleVerificationStatus::Verified))
            .count();
        let num_functions = self
            .data
            .package_data
            .values()
            .flat_map(|p| p.modules.values())
            .map(|m| m.result.function_ticks.len())
            .sum::<usize>();
        let num_functions_failed: usize = self
            .data
            .package_data
            .values()
            .flat_map(|p| p.modules.values())
            .map(|m| match &m.result.status {
                ModuleVerificationStatus::FunctionsFailed(failures) => failures.len(),
                _ => 0,
            })
            .sum();
        let num_functions_verified = num_functions - num_functions_failed;
        writeln!(
            f,
            "Packages Verified: {}/{} ({:.2}%)",
            packages_verified,
            num_packages,
            if num_packages > 0 {
                (packages_verified as f64 / num_packages as f64) * 100.0
            } else {
                100.0
            }
        )?;
        writeln!(
            f,
            "Modules Verified: {}/{} ({:.2}%)",
            modules_verified,
            num_modules,
            if num_modules > 0 {
                (modules_verified as f64 / num_modules as f64) * 100.0
            } else {
                100.0
            }
        )?;
        writeln!(
            f,
            "Functions Verified: {}/{} ({:.2}%)",
            num_functions_verified,
            num_functions,
            if num_functions > 0 {
                (num_functions_verified as f64 / num_functions as f64) * 100.0
            } else {
                100.0
            }
        )?;
        Ok(())
    }
}
