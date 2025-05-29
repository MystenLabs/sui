// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::Ok;
use move_binary_format::{
    CompiledModule,
    normalized::Bytecode::{
        self, Add, BrFalse, Branch, CopyLoc, Eq as Equal, ImmBorrowField, LdU64, Mod, MoveLoc, Mul,
        MutBorrowField, Pack, ReadRef, Ret, StLoc, WriteRef,
    },
};
use move_model::run_bytecode_model_builder;
use move_model_2::{
    model::{Model as Model2, Module},
    source_kind::{SourceKind, WithoutSource},
};
use move_stackless_bytecode::{
    function_target::FunctionTarget,
    stackless_bytecode_generator::StacklessBytecodeGenerator as OldGenerator,
};
use move_symbol_pool::Symbol;
use std::{
    collections::BTreeMap,
    fmt::{Debug, Display},
    hash::Hash,
};

use crate::ir::{
    Constant, Instruction, Operand::Var, PrimitiveOp, RValue, Var::Register,
};
use crate::utils::disassemble;

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

    pub fn legacy_disassemble(&self) -> anyhow::Result<()> {
        for module in &self.modules {
            let disassembled = disassemble(module)?;
            println!("{}", disassembled);
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

                for function in module.functions.values() {
                    let function_name = &function.name;
                    println!("\nFunction: {}", function_name);
                    let bytecode = function.code();
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
        let m_packages = self.model.packages();

        for m_package in m_packages {
            let package_name = m_package.name().unwrap_or(Symbol::from("Name not found"));
            let package_address = m_package.address();
            println!("Package: {} ({})", package_name, package_address);

            let m_modules = m_package.modules();

            for m_module in m_modules {
                let mut ctx = Context::new();

                let _ = module(&mut ctx, m_module);
            }
        }

        Ok(())
    }
}

pub struct Context {
    pub(crate) var_counter: VarCounter,
    pub(crate) ir_instructions: Vec<Instruction>,
}

impl Context {
    pub fn new() -> Self {
        Self {
            var_counter: VarCounter::new(),
            ir_instructions: Vec::new(),
        }
    }

    pub fn get_var_counter(&mut self) -> &mut VarCounter {
        &mut self.var_counter
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

fn module<K: SourceKind>(ctx: &mut Context, module: Module<K>) -> anyhow::Result<()> {
    let module = module.compiled();
    let module_name = module.name();
    let module_address = module.address();
    println!("\nModule: {} ({})", module_name, module_address);

    for function in module.functions.values() {
        let function_name = &function.name;
        println!("\nFunction: {}", function_name);
        let code = function.code();
        // TODO call the CFG and get blocks
        for op in code {
            let instruction = bytecode(ctx, &op)?;
            ctx.ir_instructions.push(instruction);
        }
    }

    for instruction in &ctx.ir_instructions {
        match instruction {
            Instruction::Return(operands) => {
                println!("Return: {:?}", operands);
            }
            Instruction::Assign { lhs, rhs } => {
                println!("Assign: Var{:?} = {:?}", lhs, rhs);
            }
            _ => {
                // Handle other instructions as needed
                println!("Instruction: {:?}", instruction);
            }
        }
    }

    Ok(())
}

fn bytecode<S: Hash + Eq + Display + Debug>(
    ctx: &mut Context,
    op: &Bytecode<S>,
) -> anyhow::Result<Instruction> {
    match op {
        // MoveLoc
        MoveLoc(loc) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctx.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::MoveLoc,
                    args: vec![Var(Register((*loc).into()))],
                },
            };
            return Ok(inst);
        }

        // CopyLoc
        CopyLoc(loc) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctx.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::CopyLoc,
                    args: vec![Var(Register((*loc).into()))],
                },
            };
            return Ok(inst);
        }

        // StoreLoc
        StLoc(loc) => {
            if ctx.var_counter.current() < 1 {
                panic!("Not enough variables to perform StLoc operation");
            }
            let inst = Instruction::Assign {
                lhs: vec![Register((*loc).into())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::StoreLoc,
                    args: vec![Var(Register(ctx.var_counter.last()))],
                },
            };
            return Ok(inst);
        }

        // ImmBorrowField
        ImmBorrowField(_field_ref) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctx.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::ImmBorrowField,
                    args: vec![Var(Register(ctx.var_counter.last()))],
                },
            };
            return Ok(inst);
        }

        // MutBorrowField
        MutBorrowField(_field_ref) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctx.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::MutBorrowField,
                    args: vec![Var(Register(ctx.var_counter.last()))],
                },
            };
            return Ok(inst);
        }

        // Pack
        Pack(_struct_ref) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctx.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Pack,
                    // TODO get how many args are needed
                    args: vec![Var(Register(ctx.var_counter.last()))],
                },
            };
            return Ok(inst);
        }

        // ReadRef
        ReadRef => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctx.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::ReadRef,
                    args: vec![Var(Register(ctx.var_counter.last()))],
                },
            };
            return Ok(inst);
        }

        // WriteRef
        WriteRef => {
            if ctx.var_counter.current() < 1 {
                panic!("Not enough variables to perform WriteRef operation");
            }
            let inst = Instruction::Assign {
                lhs: vec![(Register(ctx.var_counter.prev()))],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::WriteRef,
                    args: vec![Var(Register(ctx.var_counter.last()))],
                },
            };
            ctx.var_counter.increment();
            return Ok(inst);
        }

        // Add
        Add => {
            if ctx.var_counter.current() < 2 {
                panic!("Not enough variables to perform Add operation");
            }
            let rhs = Var(Register(ctx.var_counter.prev()));
            let lhs = Var(Register(ctx.var_counter.last()));
            ctx.var_counter.increment();
            let inst = Instruction::Assign {
                lhs: vec![Register(ctx.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Add,
                    args: vec![lhs, rhs],
                },
            };
            return Ok(inst);
        }

        // Mul
        Mul => {
            if ctx.var_counter.current() < 2 {
                panic!("Not enough variables to perform Mul operation");
            }
            let rhs = Var(Register(ctx.var_counter.prev()));
            let lhs = Var(Register(ctx.var_counter.last()));
            ctx.var_counter.increment();
            let inst = Instruction::Assign {
                lhs: vec![Register(ctx.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Multiply,
                    args: vec![lhs, rhs],
                },
            };
            return Ok(inst);
        }

        // Mod
        Mod => {
            if ctx.var_counter.current() < 2 {
                panic!("Not enough variables to perform Mod operation");
            }
            let rhs = Var(Register(ctx.var_counter.prev()));
            let lhs = Var(Register(ctx.var_counter.last()));
            ctx.var_counter.increment();
            let inst = Instruction::Assign {
                lhs: vec![Register(ctx.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Modulo,
                    args: vec![lhs, rhs],
                },
            };
            return Ok(inst);
        }

        // LdU64
        LdU64(value) => {
            // let newReg = value;
            let inst = Instruction::Assign {
                lhs: vec![Register(ctx.var_counter.next())],
                rhs: RValue::Constant(Constant::U64(*value)),
            };
            return Ok(inst);
        }

        // Ret
        Ret => {
            let inst = Instruction::Return(Var(Register(ctx.var_counter.last())));
            return Ok(inst);
        }

        _ => {
            // Handle other bytecode operations as needed
            // println!("Not implemented: {:?}", op);
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
        }
    }
}
