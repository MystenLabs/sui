// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_model_2::{
    model::Model as Model2,
    source_kind::WithoutSource
};
use move_symbol_pool::Symbol;
use move_binary_format::normalized::Bytecode::{
    MoveLoc,
    ImmBorrowField,
    ReadRef,
    Ret,
    LdU64,
    Pack,
    CopyLoc,
    Add,
    MutBorrowField,
    WriteRef
};

pub struct StacklessBytecode {
    pub(crate) model: Model2<WithoutSource>,
}


impl StacklessBytecode {
    pub fn new(model: Model2<WithoutSource>) -> Self {
        Self {
            model: model
        }
    }

    pub fn execute(&self) -> anyhow::Result<()> {
        let packages = self.model.packages();
        
        for package in packages{
            let package_name = package.name().unwrap_or(Symbol::from("Name not found"));
            let package_address = package.address();
            
            println!("Package: {} ({})", package_name, package_address);
            let modules = package.modules();
            for module in modules {
                let module = module.compiled();
                let module_name = module.name();
                let module_address = module.address();
                println!("\nModule: {} ({})", module_name, module_address);
                for dep in module.immediate_dependencies.clone() {
                    let module_name = dep.name;
                    let module_address = dep.address;
                    let import = format!("{}::{}", module_address, module_name);
                    println!("{}", import);
                }
                for strct in module.structs.values() {
                    let strct_name = strct.name;
                    println!("\nStruct: {}", strct_name);
                }
                for functn in module.functions.values() {
                    let function_name = functn.name;
                    println!("\nFunction: {}", function_name);
                    let bytecode = functn.code();
                    for op in bytecode {
                        match op {
                            // MoveLoc
                            MoveLoc(loc) => {
                                println!("TODO")
                            }
                            
                            // ImmBorrowField
                            ImmBorrowField(field_ref) => {
                                println!("TODO")
                            }

                            // ReadRef
                            ReadRef => {
                                println!("TODO")
                            }

                            // Ret
                            Ret => {
                                println!("TODO")
                            }

                            // LdU64
                            LdU64(value) => {
                                println!("TODO")
                            }
                            
                            // Pack
                            Pack(struct_ref) => {
                                println!("TODO")
                            }

                            // CopyLoc
                            CopyLoc(loc) => {
                                println!("TODO")
                            }

                            // Add
                            Add => {
                                println!("TODO")
                            }

                            // MutBorrowField
                            MutBorrowField(field_ref) => {
                                println!("TODO")
                            }

                            // WriteRef
                            WriteRef => {
                                println!("TODO")
                            }

                            _ => {
                                // Handle other bytecode operations as needed
                                println!("TODO")
                            }
                        }
                    }
                }

            }
        }

        Ok(())

    }

}