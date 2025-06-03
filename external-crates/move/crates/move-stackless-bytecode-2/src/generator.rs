// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{stackless, utils::disassemble};

use move_binary_format::{CompiledModule, normalized::Bytecode as NB};
use move_model::run_bytecode_model_builder;
use move_model_2::{
    model::Model as Model2,
    source_kind::{SourceKind, WithoutSource},
};
use move_stackless_bytecode::{
    function_target::FunctionTarget,
    stackless_bytecode_generator::StacklessBytecodeGenerator as OldGenerator,
};
use move_symbol_pool::Symbol;

use anyhow::Ok;

use std::collections::BTreeMap;

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

// TODO: Consider eliminating this struct.
pub struct StacklessBytecodeGenerator<S: SourceKind> {
    pub(crate) modules: Vec<CompiledModule>,
    pub(crate) model: Model2<S>,
}

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl StacklessBytecodeGenerator<WithoutSource> {
    pub fn new(modules: Vec<CompiledModule>) -> Self {
        Self {
            modules: modules.clone(),
            model: Model2::from_compiled(&BTreeMap::new(), modules),
        }
    }
}

impl<S: SourceKind> StacklessBytecodeGenerator<S> {
    pub fn from_model(model: Model2<S>) -> Self {
        // This is dubious, but so is holding the compiled modules instead of the normalized ones.
        let modules = vec![];
        Self { modules, model }
    }

    // TODO: Return a thing instead of printing
    pub fn legacy_stackless(&self) -> anyhow::Result<()> {
        let global_env = run_bytecode_model_builder(&self.modules)?;
        let module_envs = global_env.get_modules();
        for module_env in module_envs {
            let symbol_pool = module_env.env.symbol_pool();
            println!("Module: {}\n", module_env.get_name().display(symbol_pool));
            for function_env in module_env.get_functions() {
                let stackless_generator = OldGenerator::new(&function_env);
                let function_data = stackless_generator.generate_function();
                let function_target = FunctionTarget::new(&function_env, &function_data);
                println!("{}", function_target);
            }
        }
        Ok(())
    }

    // TODO: Return something more structured than a Vec<String>?
    pub fn legacy_disassemble(&self) -> anyhow::Result<Vec<String>> {
        let mut disassembled = Vec::new();
        for module in &self.modules {
            disassembled.push(disassemble(module)?);
        }
        Ok(disassembled)
    }

    // TODO: At some point this should hand back a set of stackless bytecode pacakges or similar --
    // look at the old interface and mirror that as a start (?)
    pub fn generate_stackless_bytecode(&self) -> anyhow::Result<Vec<stackless::ast::Package>> {
        let packages = self.model.packages();

        packages
            .into_iter()
            .map(|pkg| {
                let name = pkg
                    .name()
                    .unwrap_or_else(|| format!("{}", pkg.address()).into());
                let modules = pkg
                    .modules()
                    .map(stackless::translate::module)
                    .collect::<Result<Vec<_>, _>>()?;
                let modules = modules
                    .into_iter()
                    .map(|m| (m.name, m))
                    .collect::<BTreeMap<_, _>>();
                Ok(stackless::ast::Package { name, modules })
            })
            .collect::<Result<_, _>>()
    }

    // TODO: Return a thing instead of printing
    pub fn disassemble_source(&self) -> anyhow::Result<()> {
        let packages = self.model.packages();

        for package in packages {
            let package_name = package.name().unwrap_or(Symbol::from("Name not found"));
            let package_address = package.address();

            println!("Package: {} ({})", package_name, package_address);
            let modules = package.modules();
            for module in modules {
                let module = module.compiled();
                let module_name = module.name();
                let module_address = module.address();
                println!("\nModule: {} ({})", module_name, module_address);

                for function in module.functions.values() {
                    let function_name = &function.name;
                    println!("\nFunction: {}", function_name);
                    let bytecode = function.code();
                    for op in bytecode {
                        match op {
                            // MoveLoc
                            NB::MoveLoc(loc) => {
                                println!("MoveLoc [{}]", loc);
                            }

                            // ImmBorrowField
                            NB::ImmBorrowField(field_ref) => {
                                println!("ImmBorrowField<{}> ", field_ref.field.type_);
                            }

                            // ReadRef
                            NB::ReadRef => {
                                println!("ReadRef");
                            }

                            // Ret
                            NB::Ret => {
                                println!("Ret");
                            }

                            // LdU64
                            NB::LdU64(value) => {
                                println!("LdU64({})", value);
                            }

                            // Pack
                            NB::Pack(struct_ref) => {
                                println!("Pack<{}>", struct_ref.struct_.name);
                            }

                            // CopyLoc
                            NB::CopyLoc(loc) => {
                                println!("CopyLoc [{}]", loc);
                            }

                            // Add
                            NB::Add => {
                                println!("Add");
                            }

                            // MutBorrowField
                            NB::MutBorrowField(field_ref) => {
                                println!("MutBorrowField<{}> ", field_ref.field.type_);
                            }

                            // WriteRef
                            NB::WriteRef => {
                                println!("WriteRef");
                            }

                            _ => {
                                // Handle other bytecode operations as needed
                                println!("Bytecode: {:#?}", op);
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    // TODO: The CLI execution should be this -- so that we know printing is happening there, not
    // as part of the stackless bytecode interface itself.
    pub fn execute(&self) -> anyhow::Result<()> {
        let m_packages = self.model.packages();

        for m_package in m_packages {
            let package_name = m_package.name().unwrap_or(Symbol::from("Name not found"));
            let package_address = m_package.address();
            println!("Package: {} ({})", package_name, package_address);

            let m_modules = m_package.modules();

            for m_module in m_modules {
                println!("{}", stackless::translate::module(m_module)?);
            }
        }

        Ok(())
    }
}
