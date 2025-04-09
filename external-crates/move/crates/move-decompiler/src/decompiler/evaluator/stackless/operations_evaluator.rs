// Copyright (c) Verichains, 2023

use std::mem::MaybeUninit;

use move_model::ty::Type;
use move_stackless_bytecode::stackless_bytecode::Operation;

use super::{Expr, ExprNodeOperation, ExprNodeRef, ReturnValueHint, StacklessEvaluationContext};

pub struct OperationEvaluatorResult {
    pub cannot_keep: bool,
    pub expr: Expr,
}

pub trait OperationEvaluator {
    fn evaluate(
        &self,
        ctx: &StacklessEvaluationContext,
        args: &Vec<Expr>,
        dst_types: &Vec<Option<ReturnValueHint>>,
    ) -> Result<OperationEvaluatorResult, anyhow::Error>;
}

impl OperationEvaluator for &Operation {
    fn evaluate(
        &self,
        ctx: &StacklessEvaluationContext,
        args: &Vec<Expr>,
        dst_types: &Vec<Option<ReturnValueHint>>,
    ) -> Result<OperationEvaluatorResult, anyhow::Error> {
        fn is_mutable_reference(
            dst_types: &Vec<Option<ReturnValueHint>>,
        ) -> Result<bool, anyhow::Error> {
            if dst_types.len() != 1 {
                return Err(anyhow::anyhow!(
                    "Expected 1 return value, got {}",
                    dst_types.len()
                ));
            }

            if let Some(hint) = &dst_types[0] {
                Ok(hint.ty.is_mutable_reference())
            } else {
                Ok(false)
            }
        }

        match self {
            Operation::Function(mid, fid, types) => {
                let env = ctx.func_env.module_env.env;
                let module = env.get_module(*mid);
                let func = module.get_function(*fid);
                let has_acquire_resources = !func.get_acquires_global_resources().is_empty();

                let name = shortest_name(ctx, mid, func.get_name_str());
                Ok(OperationEvaluatorResult {
                    cannot_keep: has_acquire_resources,
                    expr: ExprNodeOperation::Func(
                        name,
                        args.iter().map(|x| x.value_copied()).collect(),
                        types.clone(),
                    )
                    .to_expr(),
                })
            }

            Operation::Unpack(mid, sid, types) | Operation::Pack(mid, sid, types) => {
                let module_env = ctx.func_env.module_env.env.get_module(*mid);
                let struct_env = module_env.get_struct(*sid);

                let name = shortest_name(ctx, mid, struct_name(&struct_env));

                let keys = struct_env
                    .get_fields()
                    .map(|x| x.get_name().display(struct_env.symbol_pool()).to_string())
                    .collect::<Vec<_>>();
                match self {
                    Operation::Unpack(..) => Ok(OperationEvaluatorResult {
                        cannot_keep: false,
                        expr: ExprNodeOperation::DatatypeUnpack(
                            name,
                            keys,
                            only_one(args, "unpack")?,
                            types.clone(),
                        )
                        .to_expr(),
                    }),

                    Operation::Pack(..) => {
                        if keys.len() != args.len() {
                            return Err(anyhow::anyhow!(
                                "Expected {} arguments, got {}",
                                keys.len(),
                                args.len(),
                            ));
                        }
                        Ok(OperationEvaluatorResult {
                            cannot_keep: false,
                            expr: ExprNodeOperation::DatatypePack(
                                name,
                                keys.iter()
                                    .zip(args.iter())
                                    .map(|(k, x)| (k.clone(), x.value_copied()))
                                    .collect(),
                                types.clone(),
                            )
                            .to_expr(),
                        })
                    }

                    _ => unreachable!(),
                }
            }

            Operation::MoveFrom(mid, sid, types)
            | Operation::MoveTo(mid, sid, types)
            | Operation::Exists(mid, sid, types)
            | Operation::BorrowGlobal(mid, sid, types) => {
                // let module_env = ctx.func_env.module_env.env.get_module(*mid);
                // let struct_env = module_env.get_struct(*sid);
                // let name = shortest_name(ctx, mid, struct_name(&struct_env));
                let (func, arg_fn): (
                    _,
                    Box<dyn Fn(&Vec<Expr>) -> Result<Vec<ExprNodeRef>, anyhow::Error>>,
                ) = match self {
                    Operation::MoveFrom(..) => (
                        "move_from",
                        Box::new(|args: &Vec<Expr>| Ok(vec![only_one(args, "move_from")?])),
                    ),

                    Operation::MoveTo(..) => (
                        "move_to",
                        Box::new(|args: &Vec<Expr>| {
                            let args = only_n::<2>(args, "move_to")?;
                            Ok(vec![args[1].clone(), args[0].clone()])
                        }),
                    ),

                    Operation::Exists(..) => (
                        "exists",
                        Box::new(|args| Ok(vec![only_one(args, "exists")?])),
                    ),

                    Operation::BorrowGlobal(..) => {
                        if is_mutable_reference(dst_types)? {
                            (
                                "borrow_global_mut",
                                Box::new(|args| Ok(vec![only_one(args, "borrow_global_mut")?])),
                            )
                        } else {
                            (
                                "borrow_global",
                                Box::new(|args| Ok(vec![only_one(args, "borrow_global")?])),
                            )
                        }
                    }

                    _ => unreachable!(),
                };

                Ok(OperationEvaluatorResult {
                    cannot_keep: false,
                    expr: ExprNodeOperation::Func(
                        format!("{}", func),
                        arg_fn(args)?,
                        vec![Type::Datatype(mid.clone(), sid.clone(), types.clone())],
                    )
                    .to_expr(),
                })
            }

            Operation::GetField(mid, sid, _targs, offset)
            | Operation::BorrowField(mid, sid, _targs, offset) => {
                let arg = only_one(args, "get_field")?;
                let module_env = ctx.func_env.module_env.env.get_module(*mid);
                let struct_env = module_env.get_struct(*sid);
                let field_env = struct_env.get_field_by_offset(*offset);
                let field_name = field_env
                    .get_name()
                    .display(struct_env.symbol_pool())
                    .to_string();

                match self {
                    Operation::GetField(..) => Ok(OperationEvaluatorResult {
                        cannot_keep: false,
                        expr: ExprNodeOperation::Field(arg, field_name).to_expr(),
                    }),

                    Operation::BorrowField(..) => Ok(OperationEvaluatorResult {
                        cannot_keep: false,
                        expr: ExprNodeOperation::BorrowLocal(
                            ExprNodeOperation::Field(arg, field_name).to_node(),
                            is_mutable_reference(dst_types)?,
                        )
                        .to_expr(),
                    }),
                    _ => unreachable!(),
                }
            }

            Operation::ReadRef => Ok(OperationEvaluatorResult {
                cannot_keep: false,
                expr: ExprNodeOperation::ReadRef(only_one(args, "read_ref")?).to_expr(),
            }),

            Operation::WriteRef => {
                let [a, b] = only_n::<2>(args, "write_ref")?;
                Ok(OperationEvaluatorResult {
                    cannot_keep: false,
                    expr: ExprNodeOperation::WriteRef(a, b).to_expr(),
                })
            }

            Operation::BorrowLoc => {
                let mutable = match &dst_types[0] {
                    Some(ReturnValueHint { ty, .. }) => ty.is_mutable_reference(),
                    _ => false,
                };
                Ok(OperationEvaluatorResult {
                    cannot_keep: false,
                    expr: ExprNodeOperation::BorrowLocal(only_one(args, "borrow local")?, mutable)
                        .to_expr(),
                })
            }

            Operation::CastU8 => cast("u8", args),
            Operation::CastU16 => cast("u16", args),
            Operation::CastU32 => cast("u32", args),
            Operation::CastU64 => cast("u64", args),
            Operation::CastU128 => cast("u128", args),
            Operation::CastU256 => cast("u256", args),
            Operation::Not => unary("!", args),
            Operation::Add => binary("+", args),
            Operation::Sub => binary("-", args),
            Operation::Mul => binary("*", args),
            Operation::Div => binary("/", args),
            Operation::Mod => binary("%", args),
            Operation::BitOr => binary("|", args),
            Operation::BitAnd => binary("&", args),
            Operation::Xor => binary("^", args),
            Operation::Shl => binary("<<", args),
            Operation::Shr => binary(">>", args),
            Operation::Lt => binary("<", args),
            Operation::Gt => binary(">", args),
            Operation::Le => binary("<=", args),
            Operation::Ge => binary(">=", args),
            Operation::Or => binary("||", args),
            Operation::And => binary("&&", args),
            Operation::Eq => binary("==", args),
            Operation::Neq => binary("!=", args),

            // specification opcode
            Operation::OpaqueCallBegin(..) | Operation::OpaqueCallEnd(..) => {
                Err(anyhow::anyhow!("OpaqueCall opcode is not supported"))
            }

            Operation::TraceLocal(_)
            | Operation::TraceReturn(_)
            | Operation::TraceAbort
            | Operation::GetGlobal(..)
            | Operation::UnpackRef
            | Operation::PackRef
            | Operation::UnpackRefDeep
            | Operation::PackRefDeep
            | Operation::Stop
            | Operation::Uninit
            | Operation::IsParent(_, _)
            | Operation::WriteBack(_, _)
            | Operation::Havoc(..) => {
                Err(anyhow::anyhow!("Specifications opcode is not supported"))
            }
            Operation::Destroy => Ok(OperationEvaluatorResult {
                cannot_keep: false,
                expr: ExprNodeOperation::Destroy(only_one(args, "destroy")?).to_expr(),
            }),
            Operation::FreezeRef => Ok(OperationEvaluatorResult {
                cannot_keep: false,
                expr: ExprNodeOperation::FreezeRef(only_one(args, "freeze_ref")?).to_expr(),
            }),
            Operation::PackVariant(_, _, _, _) => todo!(),
            Operation::UnpackVariant(_, _, _, _, _) => todo!(),
        }
    }
}

fn struct_name(struct_env: &move_model::model::StructEnv<'_>) -> String {
    struct_env
        .get_name()
        .display(struct_env.symbol_pool())
        .to_string()
}
fn shortest_name(
    ctx: &StacklessEvaluationContext<'_>,
    mid: &move_model::model::ModuleId,
    name: String,
) -> String {
    format!("{}{}", ctx.shortest_prefix(&mid), name)
}

fn only_one(args: &Vec<Expr>, operation_name: &str) -> Result<ExprNodeRef, anyhow::Error> {
    if args.len() != 1 {
        return Err(anyhow::anyhow!(
            "Expected only 1 argument for '{}'",
            operation_name
        ));
    }

    Ok(args[0].value_copied())
}

fn only_n<const N: usize>(
    args: &Vec<Expr>,
    operation_name: &str,
) -> Result<[ExprNodeRef; N], anyhow::Error> {
    if args.len() != N {
        return Err(anyhow::anyhow!(
            "Expected {} arguments for '{}'",
            N,
            operation_name
        ));
    }

    let mut r = std::mem::MaybeUninit::<[ExprNodeRef; N]>::uninit();
    let r_mut: &mut [MaybeUninit<ExprNodeRef>; N] = unsafe {
        r.as_mut_ptr()
            .cast::<[MaybeUninit<ExprNodeRef>; N]>()
            .as_mut()
            .unwrap()
    };

    for i in 0..N {
        r_mut[i] = MaybeUninit::new(args[i].value_copied());
    }

    Ok(unsafe { r.assume_init() })
}

fn unary(arg: &str, args: &Vec<Expr>) -> Result<OperationEvaluatorResult, anyhow::Error> {
    Ok(OperationEvaluatorResult {
        cannot_keep: false,
        expr: ExprNodeOperation::Unary(
            arg.to_string(),
            only_one(args, &format!("unary operation '{}'", arg))?,
        )
        .to_expr(),
    })
}

fn cast(ty: &str, args: &Vec<Expr>) -> Result<OperationEvaluatorResult, anyhow::Error> {
    Ok(OperationEvaluatorResult {
        cannot_keep: false,
        expr: ExprNodeOperation::Cast(
            ty.to_string(),
            only_one(args, &format!("unary operation '{}'", ty))?,
        )
        .to_expr(),
    })
}

fn binary(arg: &str, args: &Vec<Expr>) -> Result<OperationEvaluatorResult, anyhow::Error> {
    let [l, r] = only_n::<2>(args, &format!("binary operation '{}'", arg))?;
    Ok(OperationEvaluatorResult {
        cannot_keep: false,
        expr: ExprNodeOperation::Binary(arg.to_string(), l, r).to_expr(),
    })
}
