// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::CompiledModule;
use move_binary_format::normalized::Bytecode::{
    Add, CopyLoc, ImmBorrowField, LdU64, MoveLoc, MutBorrowField, Pack, ReadRef, Ret, WriteRef,
};
use move_model::run_bytecode_model_builder;
use move_model_2::{model::Model as Model2, source_kind::WithoutSource};
use move_stackless_bytecode::{
    function_target::FunctionTarget,
    stackless_bytecode_generator::StacklessBytecodeGenerator as OldGenerator,
};
use move_symbol_pool::Symbol;
use std::collections::BTreeMap;

use crate::stackless_ir::{Constant, Instruction, Operand, PrimitiveOp, RValue};

pub struct StacklessBytecodeGenerator {
    pub(crate) modules: Vec<CompiledModule>,
    pub(crate) model: Model2<WithoutSource>,
}

impl StacklessBytecodeGenerator {
    pub fn new(modules: Vec<CompiledModule>) -> Self {
        Self {
            modules: modules.clone(),
            model: Model2::from_compiled(&BTreeMap::new(), modules),
        }
    }

    pub fn old_stackless(&self) -> anyhow::Result<()> {
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
                                println!("MoveLoc [{}]", loc);
                            }

                            // ImmBorrowField
                            ImmBorrowField(field_ref) => {
                                println!("ImmBorrowField<{}> ", field_ref.field.type_);
                            }

                            // ReadRef
                            ReadRef => {
                                println!("ReadRef");
                            }

                            // Ret
                            Ret => {
                                println!("Ret");
                            }

                            // LdU64
                            LdU64(value) => {
                                println!("LdU64({})", value);
                            }

                            // Pack
                            Pack(struct_ref) => {
                                println!("Pack<{}>", struct_ref.struct_.name);
                            }

                            // CopyLoc
                            CopyLoc(loc) => {
                                println!("CopyLoc [{}]", loc);
                            }

                            // Add
                            Add => {
                                println!("Add");
                            }

                            // MutBorrowField
                            MutBorrowField(field_ref) => {
                                println!("MutBorrowField<{}> ", field_ref.field.type_);
                            }

                            // WriteRef
                            WriteRef => {
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

    pub fn execute(&self) -> anyhow::Result<()> {
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

                    let mut var_counter = VarCounter::new();
                    let mut ir_instrctions: Vec<Instruction> = Vec::new();
                    for op in bytecode {
                        match op {
                            // MoveLoc
                            MoveLoc(loc) => {
                                let inst = Instruction::Assign {
                                    lhs: var_counter.next(),
                                    rhs: RValue::Primitive {
                                        op: PrimitiveOp::MoveLoc,
                                        args: vec![Operand::Location(*loc)],
                                    },
                                };
                                ir_instrctions.push(inst);
                            }

                            // ImmBorrowField
                            ImmBorrowField(_field_ref) => {
                                let inst = Instruction::Assign {
                                    lhs: var_counter.next(),
                                    rhs: RValue::Primitive {
                                        op: PrimitiveOp::ImmBorrowField,
                                        args: vec![Operand::Var(var_counter.last())],
                                    },
                                };
                                ir_instrctions.push(inst);
                            }

                            // ReadRef
                            ReadRef => {
                                let inst = Instruction::Assign {
                                    lhs: var_counter.next(),
                                    rhs: RValue::Primitive {
                                        op: PrimitiveOp::ReadRef,
                                        args: vec![Operand::Var(var_counter.last())],
                                    },
                                };
                                ir_instrctions.push(inst);
                            }

                            // Ret
                            Ret => {
                                let inst =
                                    Instruction::Return(vec![Operand::Var(var_counter.last())]);
                                ir_instrctions.push(inst);
                            }

                            // LdU64
                            LdU64(value) => {
                                // let newReg = value;
                                let inst = Instruction::Assign {
                                    lhs: var_counter.next(),
                                    rhs: RValue::Constant(Constant::U64(*value)),
                                };
                                ir_instrctions.push(inst);
                            }

                            // Pack
                            Pack(_struct_ref) => {
                                let inst = Instruction::Assign {
                                    lhs: var_counter.next(),
                                    rhs: RValue::Primitive {
                                        op: PrimitiveOp::Pack,
                                        args: vec![Operand::Var(var_counter.last())],
                                    },
                                };
                                ir_instrctions.push(inst);
                            }

                            // CopyLoc
                            CopyLoc(loc) => {
                                let inst = Instruction::Assign {
                                    lhs: var_counter.next(),
                                    rhs: RValue::Primitive {
                                        op: PrimitiveOp::CopyLoc,
                                        args: vec![Operand::Location(*loc)],
                                    },
                                };
                                ir_instrctions.push(inst);
                            }

                            // Add
                            Add => {
                                if var_counter.current() < 2 {
                                    panic!("Not enough variables to perform Add operation");
                                }
                                let rhs = Operand::Var(var_counter.prev());
                                let lhs = Operand::Var(var_counter.last());
                                var_counter.increment();
                                let inst = Instruction::Assign {
                                    lhs: var_counter.next(),
                                    rhs: RValue::Primitive {
                                        op: PrimitiveOp::Add,
                                        args: vec![lhs, rhs],
                                    },
                                };
                                ir_instrctions.push(inst);
                            }

                            // MutBorrowField
                            MutBorrowField(_field_ref) => {
                                let inst = Instruction::Assign {
                                    lhs: var_counter.next(),
                                    rhs: RValue::Primitive {
                                        op: PrimitiveOp::MutBorrowField,
                                        args: vec![Operand::Var(var_counter.last())],
                                    },
                                };
                                ir_instrctions.push(inst);
                            }

                            // WriteRef
                            WriteRef => {
                                if var_counter.current() < 1 {
                                    panic!("Not enough variables to perform WriteRef operation");
                                }
                                let inst = Instruction::Assign {
                                    lhs: var_counter.prev(),
                                    rhs: RValue::Primitive {
                                        op: PrimitiveOp::WriteRef,
                                        args: vec![Operand::Var(var_counter.last())],
                                    },
                                };
                                var_counter.increment();
                                ir_instrctions.push(inst);
                            }

                            _ => {
                                // Handle other bytecode operations as needed
                                println!("TODO")
                            }
                        }
                    }
                    
                    for instruction in ir_instrctions {
                        match instruction {
                            Instruction::Return(operands) => {
                                println!("Return: {:?}", operands);
                            }
                            Instruction::Assign { lhs, rhs } => {
                                println!("Assign: Var{} = {:?}", lhs, rhs);
                            }
                            _ => {
                                // Handle other instructions as needed
                                println!("Instruction: {:?}", instruction);
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

pub struct VarCounter {
    pub(crate) count: usize,
}
impl VarCounter {
    pub fn new() -> Self {
        Self { count: 0 }
    }

    pub fn next(&mut self) -> usize {
        let current = self.count;
        self.count += 1;
        current
    }

    pub fn prev(&mut self) -> usize {
        if self.count == 0 {
            panic!("Cannot decrement VarCounter below zero");
        }
        self.count -= 1;
        self.count
    }

    pub fn reset(&mut self) {
        self.count = 0;
    }

    pub fn current(&self) -> usize {
        self.count
    }

    pub fn last(&self) -> usize {
        if self.count == 0 {
            panic!("VarCounter is empty, cannot return last value");
        }
        self.count - 1
    }

    pub fn set(&mut self, value: usize) {
        self.count = value;
    }

    pub fn increment(&mut self) {
        self.count += 1;
    }

    pub fn decrement(&mut self) {
        if self.count == 0 {
            panic!("Cannot decrement VarCounter below zero");
        }
        self.count -= 1;
    }
}

impl Default for VarCounter {
    fn default() -> Self {
        Self::new()
    }
}
