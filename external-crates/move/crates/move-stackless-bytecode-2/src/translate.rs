// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ast::{self, BasicBlock, Instruction, RValue, Register, Value},
    optimizations::optimize,
};

use move_abstract_interpreter::control_flow_graph::{ControlFlowGraph, VMControlFlowGraph};
use move_binary_format::{
    file_format::JumpTableInner,
    normalized::{self as N, Bytecode as IB, Type},
};

use move_model_2::{
    model::{Model, Module, Package},
    source_kind::SourceKind,
};
use move_symbol_pool::Symbol;

use std::{collections::BTreeMap, rc::Rc, result::Result::Ok};

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

pub struct Context<'a, K: SourceKind> {
    pub var_counter: Counter,
    pub model: &'a Model<K>,
    pub logical_stack: Vec<Register>,
    pub optimize: bool,
    pub locals_types: Vec<Rc<Type<Symbol>>>,
}

pub struct Counter {
    pub count: usize,
}

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl<'a, K: SourceKind> Context<'a, K> {
    pub fn new(model: &'a Model<K>) -> Self {
        Self {
            var_counter: Counter::new(),
            model,
            logical_stack: vec![],
            optimize: false,
            locals_types: vec![],
        }
    }

    pub fn pop_register(&mut self) -> Register {
        self.logical_stack
            .pop()
            .expect("Popped a register and there was none")
    }

    pub fn push_register(&mut self, ty: Rc<Type<Symbol>>) -> Register {
        let reg_id = self.var_counter.incr();
        let new_reg = Register { name: reg_id, ty };
        self.logical_stack.push(new_reg.clone());
        new_reg
    }

    #[allow(dead_code)]
    pub fn nth_register(&self, n: usize) -> &Register {
        self.logical_stack
            .get(self.logical_stack.len() - n)
            .expect("Tried to get nth register but stack is too small")
    }

    pub fn optimize(&mut self, value: bool) {
        self.optimize = value;
    }

    pub fn set_locals_types(&mut self, locals_types: Vec<Rc<Type<Symbol>>>) {
        self.locals_types = locals_types;
    }

    pub fn get_local_type(&self, loc: usize) -> &Rc<Type<Symbol>> {
        &self.locals_types[loc]
    }
}

impl Counter {
    pub fn new() -> Self {
        Self { count: 0 }
    }

    pub fn incr(&mut self) -> usize {
        let curr = self.count;
        self.count += 1;
        curr
    }

    pub fn reset(&mut self) {
        self.count = 0;
    }
}

// -------------------------------------------------------------------------------------------------
// Default
// -------------------------------------------------------------------------------------------------

impl Default for Counter {
    fn default() -> Self {
        Self::new()
    }
}

// -------------------------------------------------------------------------------------------------
// Stackless Bytecode Translation
// -------------------------------------------------------------------------------------------------

pub(crate) fn packages<K: SourceKind>(
    model: &Model<K>,
    optimize: bool,
) -> anyhow::Result<Vec<ast::Package>> {
    let mut context = Context::new(model);
    context.optimize(optimize);
    let mut packages = vec![];
    let m_packages = model.packages();
    for m_package in m_packages {
        let package = package(&mut context, m_package)?;
        packages.push(package);
    }
    Ok(packages)
}

pub(crate) fn package<K: SourceKind>(
    context: &mut Context<'_, K>,
    package: Package<K>,
) -> anyhow::Result<ast::Package> {
    let package_name = package.name();
    let package_address = package.address();

    let m_modules = package.modules();

    let out_modules = m_modules
        .into_iter()
        .map(|m_module| module(context, m_module))
        .collect::<Result<Vec<_>, _>>()?;

    let package = ast::Package {
        name: package_name,
        address: package_address,
        modules: out_modules.into_iter().map(|m| (m.name, m)).collect(),
    };
    Ok(package)
}

pub(crate) fn module<K: SourceKind>(
    context: &mut Context<'_, K>,
    module: Module<K>,
) -> anyhow::Result<ast::Module> {
    let module = module.compiled();
    let name = *module.name();

    let mut functions = BTreeMap::new();

    for fun in module.functions.values() {
        context.var_counter.reset();
        let function_name = fun.name;
        functions.insert(function_name, function(context, fun)?);
    }
    let module = ast::Module { name, functions };

    Ok(module)
}

pub(crate) fn function<K: SourceKind>(
    ctxt: &mut Context<'_, K>,
    function: &N::Function<Symbol>,
) -> anyhow::Result<ast::Function> {
    let name = function.name;
    let code = function.code();
    if code.is_empty() {
        return Ok(ast::Function {
            name,
            entry_label: 0,
            basic_blocks: BTreeMap::new(),
        });
    }
    let cfg = VMControlFlowGraph::new(code, function.jump_tables());

    let locals_types = function
        .parameters
        .iter()
        .chain(function.locals.iter())
        .cloned()
        .collect::<Vec<_>>();
    ctxt.set_locals_types(locals_types);

    let mut basic_blocks = BTreeMap::new();

    for block_id in cfg.blocks() {
        let blk_start = cfg.block_start(block_id);
        let blk_end = cfg.block_end(block_id);
        let code_range = &code[blk_start as usize..(blk_end + 1) as usize];
        let block_instructions = code_range
            .iter()
            .enumerate()
            .map(|(i, op)| bytecode(ctxt, op, blk_start as usize + i, function))
            .collect::<Vec<_>>();

        let label = block_id as usize;
        let bb = BasicBlock::from_instructions(label, block_instructions);
        if !ctxt.logical_stack.is_empty() {
            panic!("Logical stack not empty: {:#?}\n{}", ctxt.logical_stack, bb);
        }
        basic_blocks.insert(label, bb);
    }

    let mut function = ast::Function {
        name,
        entry_label: cfg.entry_block_id() as usize,
        basic_blocks,
    };

    if ctxt.optimize {
        optimize(&mut function)
    }

    Ok(function)
}

pub(crate) fn bytecode<K: SourceKind>(
    ctxt: &mut Context<'_, K>,
    op: &IB<Symbol>,
    pc: usize,
    function: &N::Function<Symbol>,
) -> Instruction {
    use N::Type;
    use ast::DataOp;
    use ast::LocalOp as LocOp;
    use ast::PrimitiveOp as Op;

    use ast::Trivial::Immediate as I;
    use ast::Trivial::Register as R;

    macro_rules! assign_reg {
        ([$($lhs:expr),*] = $rhs:expr) => {{
            let rhs = $rhs;
            Instruction::AssignReg {
                lhs: vec![$($lhs),*],
                rhs,
            }
        }};
    }

    macro_rules! imm {
        ($val:expr) => {
            RValue::Trivial(I($val))
        };
    }

    macro_rules! primitive_op {
        // Require at least one arg, allow a trailing comma
        ($op:expr, $($rval:expr),+ $(,)?) => {
            RValue::Primitive {
                op: $op,
                args: vec![$($rval),*],
            }
        };
    }

    macro_rules! data_op {
        ($op:expr, $($rval:expr),+ $(,)?) => {
            RValue::Data {
                op: $op,
                args: vec![$($rval),+],
            }
        };
    }

    macro_rules! make_vec {
        ($n:expr, $e:expr) => {{ (0..$n).map(|_| $e).collect::<Vec<_>>() }};
    }

    macro_rules! push {
        ($ty:expr) => {
            ctxt.push_register($ty)
        };
    }

    macro_rules! pop {
        () => {
            ctxt.pop_register()
        };
    }

    macro_rules! binary_op_type_assert {
        ($reg:expr, $other:expr) => {
            assert!(
                $reg.ty.eq(&$other.ty),
                "Type mismatch: {:?} vs {:?}",
                $reg.ty,
                $other.ty
            )
        };
    }

    macro_rules! binop {
        ($op:expr, $lhs:ident => $ty:expr) => {{
            let rhs = pop!();
            let $lhs = pop!();
            binary_op_type_assert!($lhs, rhs);
            let ty = $ty;
            assign_reg!([push!(ty)] = primitive_op!($op, R($lhs), R(rhs)))
        }};
    }

    // TODO: Everywhere we call pop!() we should check that the type is what we expect
    // TODO: Almost everywhere we call pop!() appears to be out of order -- we should reverse the
    // order we grab inputs.

    match op {
        IB::Pop => Instruction::Drop(pop!()),

        IB::Ret => {
            // TODO: check if this needs to be reversed?
            let returned_vars = make_vec!(function.return_.len(), R(pop!()));
            Instruction::Return(returned_vars)
        }

        IB::BrTrue(code_offset) => Instruction::JumpIf {
            condition: R(pop!()),
            then_label: *code_offset as usize,
            else_label: pc + 1,
        },

        IB::BrFalse(code_offset) => Instruction::JumpIf {
            condition: R(pop!()),
            then_label: pc + 1,
            else_label: *code_offset as usize,
        },

        IB::Branch(code_offset) => Instruction::Jump(*code_offset as usize),

        IB::CastU8 => {
            assign_reg!([push!(Type::U8.into())] = primitive_op!(Op::CastU8, R(pop!())))
        }

        IB::CastU64 => {
            assign_reg!([push!(Type::U64.into())] = primitive_op!(Op::CastU64, R(pop!())))
        }

        IB::CastU128 => {
            assign_reg!([push!(Type::U128.into())] = primitive_op!(Op::CastU128, R(pop!())))
        }

        IB::LdConst(const_ref) => assign_reg!(
            [push!(const_ref.type_.clone().into())] = RValue::Constant(const_ref.clone())
        ),

        IB::LdTrue => assign_reg!([push!(Type::Bool.into())] = imm!(Value::Bool(true))),

        IB::LdFalse => assign_reg!([push!(Type::Bool.into())] = imm!(Value::Bool(false))),

        IB::CopyLoc(loc) => {
            let local_idx = *loc as usize;
            let local_type = ctxt.get_local_type(local_idx).clone();
            assign_reg!(
                [push!(local_type)] = RValue::Local {
                    op: LocOp::Copy,
                    arg: local_idx
                }
            )
        }

        IB::MoveLoc(loc) => {
            let local_idx = *loc as usize;
            let local_type = ctxt.get_local_type(local_idx).clone();
            assign_reg!(
                [push!(local_type)] = RValue::Local {
                    op: LocOp::Move,
                    arg: local_idx
                }
            )
        }

        IB::StLoc(loc) => {
            let reg = pop!();
            Instruction::StoreLoc {
                loc: *loc as usize,
                value: R(reg),
            }
        }

        IB::Call(function_ref) => {
            let name = &function_ref.module.name;
            let mut modules = ctxt.model.modules();
            let module = modules
                .find(|m| {
                    m.compiled().name() == (&function_ref.module.name)
                        && *m.compiled().address() == function_ref.module.address
                })
                .unwrap_or_else(|| {
                    panic!(
                        "Module {} with address {} not found in the model",
                        name, function_ref.module.address
                    )
                });
            let compiled = module.compiled();
            let function = compiled
                .functions
                .get(&function_ref.function)
                .unwrap_or_else(|| {
                    panic!(
                        "Function {} not found in module {}",
                        function_ref.function, name
                    )
                });

            let args = make_vec!(function.parameters.len(), R(pop!()));

            let type_params = function_ref
                .type_arguments
                .iter()
                .map(|ty| ty.as_ref().clone())
                .collect::<Vec<_>>();
            let lhs = function
                .return_
                .iter()
                .map(|ty| push!(ty.clone().subst(&type_params).into()))
                .collect::<Vec<_>>();

            let target = (function_ref.module, function.name);
            Instruction::AssignReg {
                lhs,
                rhs: RValue::Call { target, args },
            }
        }

        IB::Pack(struct_ref) => {
            let args = make_vec!(struct_ref.struct_.fields.0.len(), R(pop!()));
            assign_reg!(
                [push!(struct_ref_to_type(struct_ref).into())] = RValue::Data {
                    op: DataOp::Pack(struct_ref.clone()),
                    args
                }
            )
        }

        IB::Unpack(struct_ref) => {
            let rhs = RValue::Data {
                op: DataOp::Unpack(struct_ref.clone()),
                args: vec![R(pop!())],
            };
            let lhs = struct_ref
                .struct_
                .fields
                .0
                .iter()
                .map(|(_, field)| push!(field.type_.clone().into()))
                .collect::<Vec<_>>();

            Instruction::AssignReg { rhs, lhs }
        }

        IB::ReadRef => {
            let reg = pop!();
            match reg.ty.as_ref() {
                Type::Reference(_mutable, ty) => {
                    assign_reg!(
                        [push!(ty.clone().into())] = data_op!(DataOp::ReadRef, R(reg.clone()))
                    )
                }
                _ => panic!("ReadRef expected a reference type, got: {}", reg.ty),
            }
        }

        IB::WriteRef => {
            let reg = pop!();
            let val = pop!();
            match reg.ty.as_ref() {
                Type::Reference(_mutable, ty) => {
                    assert!(
                        (**ty).eq(&(*val.ty)),
                        "Type mismatch: {:?} vs {:?}",
                        ty,
                        val.ty
                    );
                    assign_reg!([] = data_op!(DataOp::WriteRef, R(reg.clone()), R(val.clone())))
                }
                _ => panic!("WriteRef expected a reference type, got: {}", reg.ty),
            }
        }

        IB::FreezeRef => {
            let reg = pop!();
            match reg.ty.as_ref() {
                Type::Reference(true, ty) => {
                    assign_reg!(
                        [push!(Type::Reference(false, ty.clone()).into())] =
                            data_op!(DataOp::FreezeRef, R(reg.clone()))
                    )
                }
                _ => panic!(
                    "FreezeRef expected a mutable reference type, got: {}",
                    reg.ty
                ),
            }
        }

        IB::MutBorrowLoc(loc) => {
            let local_idx = *loc as usize;
            let local_type = ctxt.get_local_type(local_idx).as_ref().clone();
            let ref_type = Type::Reference(true, local_type.into());
            assign_reg!(
                [push!(ref_type.into())] = RValue::Local {
                    op: LocOp::Borrow(ast::Mutability::Mutable),
                    arg: local_idx
                }
            )
        }

        IB::ImmBorrowLoc(loc) => {
            let local_idx = *loc as usize;
            let local_type = ctxt.get_local_type(local_idx).as_ref().clone();
            let ref_type = Type::Reference(false, local_type.into());
            assign_reg!(
                [push!(ref_type.into())] = RValue::Local {
                    op: LocOp::Borrow(ast::Mutability::Immutable),
                    arg: local_idx
                }
            )
        }

        IB::MutBorrowField(field_ref) => {
            let ref_type = Type::Reference(true, field_ref.field.type_.clone().into());
            assign_reg!(
                [push!(ref_type.into())] =
                    data_op!(DataOp::MutBorrowField(field_ref.clone()), R(pop!()))
            )
        }

        IB::ImmBorrowField(field_ref) => {
            let ref_type = Type::Reference(false, field_ref.field.type_.clone().into());
            assign_reg!(
                [push!(ref_type.into())] =
                    data_op!(DataOp::ImmBorrowField(field_ref.clone()), R(pop!()))
            )
        }

        IB::Add => binop!(Op::Add, lhs => lhs.ty.clone()),
        IB::Sub => binop!(Op::Subtract, lhs => lhs.ty.clone()),
        IB::Mod => binop!(Op::Modulo, lhs => lhs.ty.clone()),
        IB::Mul => binop!(Op::Multiply, lhs => lhs.ty.clone()),
        IB::Div => binop!(Op::Divide, lhs => lhs.ty.clone()),

        IB::BitAnd => binop!(Op::BitAnd, lhs => N::Type::Bool.into()),
        IB::BitOr => binop!(Op::BitOr, lhs => N::Type::Bool.into()),
        IB::Xor => binop!(Op::Xor, lhs => N::Type::Bool.into()),
        IB::And => binop!(Op::And, lhs => N::Type::Bool.into()),
        IB::Or => binop!(Op::Or, lhs => N::Type::Bool.into()),
        IB::Eq => binop!(Op::Equal, lhs => N::Type::Bool.into()),
        IB::Neq => binop!(Op::NotEqual, lhs => N::Type::Bool.into()),
        IB::Lt => binop!(Op::LessThan, lhs => N::Type::Bool.into()),
        IB::Gt => binop!(Op::GreaterThan, lhs => N::Type::Bool.into()),
        IB::Le => binop!(Op::LessThanOrEqual, lhs => N::Type::Bool.into()),
        IB::Ge => binop!(Op::GreaterThanOrEqual, lhs => N::Type::Bool.into()),

        IB::Shl => {
            // TODO: Check the types
            let rhs = pop!();
            let lhs = pop!();
            let ty = lhs.ty.clone();
            assign_reg!([push!(ty)] = primitive_op!((Op::ShiftLeft), R(lhs), R(rhs)))
        }

        IB::Shr => {
            // TODO: Check the types
            let rhs = pop!();
            let lhs = pop!();
            let ty = lhs.ty.clone();
            assign_reg!([push!(ty)] = primitive_op!((Op::ShiftRight), R(lhs), R(rhs)))
        }

        IB::Not => {
            let reg = pop!();
            assign_reg!([push!(reg.ty.clone())] = primitive_op!(Op::Not, R(reg.clone())))
        }

        IB::Abort => Instruction::Abort(R(ctxt.pop_register())),

        IB::Nop => Instruction::Nop,

        IB::VecPack(bx) => {
            let mut args = vec![];
            for _ in 0..bx.1 {
                args.push(R(pop!()));
            }
            assign_reg!(
                [push!(bx.0.clone())] = RValue::Data {
                    op: DataOp::VecPack(bx.0.clone()),
                    args,
                }
            )
        }

        IB::VecLen(rc_type) => {
            assign_reg!(
                [push!(Type::U64.into())] = data_op!(DataOp::VecLen(rc_type.clone()), R(pop!()))
            )
        }

        IB::VecImmBorrow(rc_type) => {
            let ref_type = Type::Reference(false, rc_type.as_ref().clone().into());
            assign_reg!(
                [push!(ref_type.into())] =
                    data_op!(DataOp::VecImmBorrow(rc_type.clone()), R(pop!()), R(pop!()))
            )
        }

        IB::VecMutBorrow(rc_type) => {
            let ref_type = Type::Reference(true, rc_type.as_ref().clone().into());
            assign_reg!(
                [push!(ref_type.into())] =
                    data_op!(DataOp::VecMutBorrow(rc_type.clone()), R(pop!()), R(pop!()))
            )
        }

        IB::VecPushBack(rc_type) => {
            assign_reg!([] = data_op!(DataOp::VecPushBack(rc_type.clone()), R(pop!()), R(pop!())))
        }

        IB::VecPopBack(rc_type) => {
            assign_reg!(
                [push!(rc_type.clone())] = data_op!(DataOp::VecPopBack(rc_type.clone()), R(pop!()))
            )
        }

        IB::VecUnpack(bx) => {
            let rhs = data_op!(DataOp::VecUnpack(bx.0.clone()), R(pop!()));
            let mut lhs = vec![];
            // Actually VecUnpack is only generated on empty vectors, so bx.1 is always 0
            for _i in 0..bx.1 {
                lhs.push(push!(bx.0.clone()));
            }
            Instruction::AssignReg { rhs, lhs }
        }

        IB::VecSwap(rc_type) => {
            let args = make_vec!(3, R(pop!()));
            Instruction::AssignReg {
                rhs: RValue::Data {
                    op: DataOp::VecSwap(rc_type.clone()),
                    args,
                },
                lhs: vec![],
            }
        }

        IB::LdU8(value) => assign_reg!([push!(Type::U8.into())] = imm!(Value::U8(*value))),
        IB::LdU16(value) => assign_reg!([push!(Type::U16.into())] = imm!(Value::U16(*value))),
        IB::LdU32(value) => assign_reg!([push!(Type::U32.into())] = imm!(Value::U32(*value))),
        IB::LdU64(value) => assign_reg!([push!(Type::U64.into())] = imm!(Value::U64(*value))),
        IB::LdU128(bx) => assign_reg!([push!(Type::U128.into())] = imm!(Value::U128(*(*bx)))),
        IB::LdU256(_bx) => assign_reg!([push!(Type::U256.into())] = imm!(Value::U256(*(*_bx)))),

        IB::CastU16 => {
            assign_reg!([push!(Type::U16.into())] = primitive_op!(Op::CastU16, R(pop!())))
        }

        IB::CastU32 => {
            assign_reg!([push!(Type::U32.into())] = primitive_op!(Op::CastU32, R(pop!())))
        }

        IB::CastU256 => {
            assign_reg!([push!(Type::U256.into())] = primitive_op!(Op::CastU256, R(pop!())))
        }

        IB::PackVariant(bx) => {
            let args = make_vec!(bx.variant.fields.0.len(), R(pop!()));
            Instruction::AssignReg {
                lhs: vec![push!(variant_ref_to_type(bx).into())],
                rhs: RValue::Data {
                    op: DataOp::PackVariant(bx.clone()),
                    args,
                },
            }
        }

        IB::UnpackVariant(bx) => {
            let rhs = RValue::Data {
                op: DataOp::UnpackVariant(bx.clone()),
                args: vec![R(pop!())],
            };
            let lhs = make_vec!(
                bx.variant.fields.0.len(),
                push!(variant_ref_to_type(bx).into())
            );
            Instruction::AssignReg { lhs, rhs }
        }

        IB::UnpackVariantImmRef(bx) => {
            let rhs = RValue::Data {
                op: DataOp::UnpackVariantImmRef(bx.clone()),
                args: vec![R(pop!())],
            };
            let ref_type = Type::Reference(false, variant_ref_to_type(bx).into());
            let lhs = make_vec!(bx.variant.fields.0.len(), push!(ref_type.clone().into()));
            Instruction::AssignReg { lhs, rhs }
        }

        IB::UnpackVariantMutRef(bx) => {
            let rhs = RValue::Data {
                op: DataOp::UnpackVariant(bx.clone()),
                args: vec![R(pop!())],
            };
            let ref_type = Type::Reference(true, variant_ref_to_type(bx).into());
            let lhs = make_vec!(bx.variant.fields.0.len(), push!(ref_type.clone().into()));
            Instruction::AssignReg { lhs, rhs }
        }

        IB::VariantSwitch(jt) => {
            let JumpTableInner::Full(offsets) = &jt.jump_table;
            let enum_ = (jt.enum_.defining_module, jt.enum_.name);
            Instruction::VariantSwitch {
                condition: R(pop!()),
                enum_,
                variants: jt.enum_.variants.keys().cloned().collect(),
                labels: offsets
                    .iter()
                    .map(|offset| *offset as usize)
                    .collect::<Vec<_>>(),
            }
        }

        // ******** DEPRECATED BYTECODES ********
        IB::MutBorrowGlobalDeprecated(_bx) => Instruction::NotImplemented(format!("{:?}", op)),
        IB::ImmBorrowGlobalDeprecated(_bx) => Instruction::NotImplemented(format!("{:?}", op)),
        IB::ExistsDeprecated(_bx) => Instruction::NotImplemented(format!("{:?}", op)),
        IB::MoveFromDeprecated(_bx) => Instruction::NotImplemented(format!("{:?}", op)),
        IB::MoveToDeprecated(_bx) => Instruction::NotImplemented(format!("{:?}", op)),
    }
}

fn struct_ref_to_type(struct_ref: &N::StructRef<Symbol>) -> N::Type<Symbol> {
    let signature = (*struct_ref.type_arguments)
        .iter()
        .map(|ty| ty.as_ref().clone())
        .collect::<Vec<_>>();
    let dty = struct_ref
        .struct_
        .datatype(signature)
        .expect("Wrong datatype in struct reference");
    N::Type::Datatype(dty.into())
}

fn variant_ref_to_type(variant_ref: &N::VariantRef<Symbol>) -> N::Type<Symbol> {
    let signature = variant_ref
        .instantiation
        .iter()
        .map(|ty| ty.as_ref().clone())
        .collect::<Vec<_>>();
    let dty = variant_ref
        .enum_
        .datatype(signature)
        .expect("Wrong datatype in variant reference");
    N::Type::Datatype(dty.into())
}
