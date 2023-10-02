// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::{Location, PartialVMResult, VMResult};

use move_binary_format::{
    errors::PartialVMError,
    file_format::{Ability, AbilitySet, Bytecode},
};
use move_core_types::vm_status::StatusCode;
use move_vm_types::{loaded_data::runtime_types::Type, values::Locals};

use crate::{
    interpreter::{check_ability, FrameInterface, InstrRet, InterpreterInterface},
    loader::{Function, Resolver},
    plugin::InterpreterHook,
};

struct TypeStack {
    types: Vec<Type>,
}

impl TypeStack {
    /// Create a new empty operand stack.
    fn new() -> Self {
        TypeStack { types: vec![] }
    }

    /// Push a `Value` on the stack.
    fn push_ty(&mut self, ty: Type) -> () {
        self.types.push(ty);
    }

    /// Pop a `Value` off the stack or abort execution if the stack is empty.
    fn pop_ty(&mut self) -> PartialVMResult<Type> {
        self.types
            .pop()
            .ok_or_else(|| PartialVMError::new(StatusCode::EMPTY_VALUE_STACK))
    }

    /// Pop n values off the stack.
    fn popn_tys(&mut self, n: u16) -> PartialVMResult<Vec<Type>> {
        let remaining_stack_size = self
            .types
            .len()
            .checked_sub(n as usize)
            .ok_or_else(|| PartialVMError::new(StatusCode::EMPTY_VALUE_STACK))?;
        let args = self.types.split_off(remaining_stack_size);
        Ok(args)
    }

    fn check_balance(&self, interpreter: &dyn InterpreterInterface) -> PartialVMResult<()> {
        if self.types.len() != interpreter.get_stack_len() {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR).with_message(
                    "Paranoid Mode: Type and value stack need to be balanced".to_string(),
                ),
            );
        }
        Ok(())
    }
}

pub struct ParanoidTypeChecker {
    type_stack: TypeStack,
}

impl InterpreterHook for ParanoidTypeChecker {
    fn is_critical(&self) -> bool {
        true
    }

    fn pre_entrypoint(
        &mut self,
        function: &Function,
        ty_args: &[Type],
        resolver: &Resolver,
    ) -> VMResult<()> {
        if function.is_native() {
            self.push_parameter_types(&function, &ty_args, &resolver)
                .map_err(|e| e.finish(Location::Undefined))?;
            self.check_parameter_types(&function, ty_args, &resolver)
                .map_err(|e| match function.module_id() {
                    Some(id) => e
                        .at_code_offset(function.index(), 0)
                        .finish(Location::Module(id.clone())),
                    None => PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(
                            "Unexpected native function not located in a module".to_owned(),
                        )
                        .finish(Location::Undefined),
                })?;
        }
        Ok(())
    }

    fn post_entrypoint(&mut self) -> VMResult<()> {
        Ok(())
    }

    fn pre_fn(
        &mut self,
        interpreter: &dyn InterpreterInterface,
        current_frame: &dyn FrameInterface,
        function: &Function,
        ty_args: Option<&[Type]>,
        resolver: &Resolver,
    ) -> VMResult<()> {
        let ty_args = ty_args.unwrap_or_else(|| current_frame.get_ty_args());
        self.check_friend_or_private_call(current_frame.function(), &function)
            .map_err(|e| interpreter.set_location(e))?;
        if function.is_native() {
            self.native_function(&function, ty_args, &resolver)
                .map_err(|e| match function.module_id() {
                    Some(id) => {
                        let e = if resolver.loader().vm_config().error_execution_state {
                            e.with_exec_state(interpreter.get_internal_state())
                        } else {
                            e
                        };
                        e.at_code_offset(function.index(), 0)
                            .finish(Location::Module(id.clone()))
                    }
                    None => {
                        let err =
                            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                                .with_message(
                                    "Unexpected native function not located in a module".to_owned(),
                                );
                        interpreter.set_location(err)
                    }
                })?;
        } else {
            self.non_native_function(&function, ty_args, &resolver)
                .map_err(|e| interpreter.set_location(e))
                .map_err(|err| interpreter.maybe_core_dump(err, current_frame.get_frame()))?;
        }
        Ok(())
    }

    fn post_fn(&mut self, _function: &Function) -> VMResult<()> {
        Ok(())
    }

    fn pre_instr(
        &mut self,
        interpreter: &dyn InterpreterInterface,
        function: &Function,
        instruction: &Bytecode,
        locals: &Locals,
        ty_args: &[Type],
        resolver: &Resolver,
    ) -> PartialVMResult<()> {
        let local_tys = self.get_local_types(&resolver, &function, &ty_args)?;

        self.pre_instr(
            interpreter,
            &local_tys,
            locals,
            ty_args,
            resolver,
            instruction,
        )?;
        Ok(())
    }

    fn post_instr(
        &mut self,
        interpreter: &dyn InterpreterInterface,
        function: &Function,
        instruction: &Bytecode,
        ty_args: &[Type],
        resolver: &Resolver,
        r: &InstrRet,
    ) -> PartialVMResult<()> {
        let local_tys = self.get_local_types(&resolver, &function, &ty_args)?;

        self.post_instr(interpreter, &local_tys, ty_args, resolver, instruction, r)?;
        Ok(())
    }
}

impl ParanoidTypeChecker {
    pub fn new() -> Self {
        Self {
            type_stack: TypeStack::new(),
        }
    }

    fn push_parameter_types(
        &mut self,
        function: &Function,
        ty_args: &[Type],
        resolver: &Resolver,
    ) -> PartialVMResult<()> {
        for ty in function.parameter_types() {
            let type_ = if ty_args.is_empty() {
                ty.clone()
            } else {
                resolver.subst(ty, &ty_args)?
            };
            self.type_stack.push_ty(type_);
        }
        Ok(())
    }

    fn check_friend_or_private_call(
        &self,
        caller: &Function,
        callee: &Function,
    ) -> PartialVMResult<()> {
        if callee.is_friend_or_private() {
            match (caller.module_id(), callee.module_id()) {
                (Some(caller_id), Some(callee_id)) => {
                    if caller_id.address() == callee_id.address() {
                        Ok(())
                    } else {
                        Err(PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                                .with_message(
                                    format!("Private/Friend function invocation error, caller: {:?}::{:?}, callee: {:?}::{:?}", caller_id, caller.name(), callee_id, callee.name()),
                                ))
                    }
                }
                _ => Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(format!(
                            "Private/Friend function invocation error caller: {:?}, callee {:?}",
                            caller.name(),
                            callee.name()
                        )),
                ),
            }
        } else {
            Ok(())
        }
    }

    fn non_native_function(
        &mut self,
        function: &Function,
        ty_args: &[Type],
        resolver: &Resolver,
    ) -> PartialVMResult<()> {
        self.check_local_types(&function, &ty_args, &resolver)?;
        self.get_local_types(&resolver, &function, &ty_args)?;
        Ok(())
    }

    fn check_local_types(
        &mut self,
        function: &Function,
        ty_args: &[Type],
        resolver: &Resolver,
    ) -> PartialVMResult<()> {
        let arg_count = function.arg_count();
        let is_generic = !ty_args.is_empty();

        for i in 0..arg_count {
            let ty = self.type_stack.pop_ty()?;
            if is_generic {
                ty.check_eq(
                    &resolver.subst(&function.local_types()[arg_count - i - 1], &ty_args)?,
                )?;
            } else {
                // Directly check against the expected type to save a clone here.
                ty.check_eq(&function.local_types()[arg_count - i - 1])?;
            }
        }
        Ok(())
    }

    fn get_local_types(
        &self,
        resolver: &Resolver,
        function: &Function,
        ty_args: &[Type],
    ) -> PartialVMResult<Vec<Type>> {
        Ok(if ty_args.is_empty() {
            function.local_types().to_vec()
        } else {
            function
                .local_types()
                .iter()
                .map(|ty| resolver.subst(ty, &ty_args))
                .collect::<PartialVMResult<Vec<_>>>()?
        })
    }

    fn native_function(
        &mut self,
        function: &Function,
        ty_args: &[Type],
        resolver: &Resolver,
    ) -> PartialVMResult<()> {
        self.check_parameter_types(&function, ty_args, &resolver)?;
        self.push_return_types(&function, &ty_args)?;
        Ok(())
    }

    fn check_parameter_types(
        &mut self,
        function: &Function,
        ty_args: &[Type],
        resolver: &Resolver,
    ) -> PartialVMResult<()> {
        let expected_args = function.arg_count();
        for i in 0..expected_args {
            let expected_ty =
                resolver.subst(&function.parameter_types()[expected_args - i - 1], ty_args)?;
            let ty = self.type_stack.pop_ty()?;
            ty.check_eq(&expected_ty)?;
        }
        Ok(())
    }

    fn push_return_types(&mut self, function: &Function, ty_args: &[Type]) -> PartialVMResult<()> {
        for ty in function.return_types() {
            self.type_stack.push_ty(ty.subst(ty_args)?);
        }
        Ok(())
    }

    fn pre_instr(
        &mut self,
        interpreter: &dyn InterpreterInterface,
        local_tys: &[Type],
        locals: &Locals,
        ty_args: &[Type],
        resolver: &Resolver,
        instruction: &Bytecode,
    ) -> PartialVMResult<()> {
        self.type_stack.check_balance(interpreter)?;
        self.pre_execution_type_stack_transition(
            local_tys,
            locals,
            ty_args,
            resolver,
            instruction,
        )?;
        Ok(())
    }

    fn post_instr(
        &mut self,
        interpreter: &dyn InterpreterInterface,
        local_tys: &[Type],
        ty_args: &[Type],
        resolver: &Resolver,
        instruction: &Bytecode,
        r: &InstrRet,
    ) -> PartialVMResult<()> {
        if let InstrRet::Ok = r {
            self.post_execution_type_stack_transition(local_tys, ty_args, resolver, instruction)?;

            self.type_stack.check_balance(interpreter)?;
        }
        Ok(())
    }

    /// Paranoid type checks to perform before instruction execution.
    ///
    /// Note that most of the checks should happen after instruction execution, because gas charging will happen during
    /// instruction execution and we want to avoid running code without charging proper gas as much as possible.
    fn pre_execution_type_stack_transition(
        &mut self,
        local_tys: &[Type],
        locals: &Locals,
        _ty_args: &[Type],
        resolver: &Resolver,
        instruction: &Bytecode,
    ) -> PartialVMResult<()> {
        match instruction {
            // Call instruction will be checked at execute_main.
            Bytecode::Call(_) | Bytecode::CallGeneric(_) => (),
            Bytecode::BrFalse(_) | Bytecode::BrTrue(_) => {
                self.type_stack.pop_ty()?;
            }
            Bytecode::Branch(_) => (),
            Bytecode::Ret => {
                for (idx, ty) in local_tys.iter().enumerate() {
                    if !locals.is_invalid(idx)? {
                        check_ability(resolver.loader().abilities(ty)?.has_drop())?;
                    }
                }
            }
            Bytecode::Abort => {
                self.type_stack.pop_ty()?;
            }
            // StLoc needs to check before execution as we need to check the drop ability of values.
            Bytecode::StLoc(idx) => {
                let ty = local_tys[*idx as usize].clone();
                let val_ty = self.type_stack.pop_ty()?;
                ty.check_eq(&val_ty)?;
                if !locals.is_invalid(*idx as usize)? {
                    check_ability(resolver.loader().abilities(&ty)?.has_drop())?;
                }
            }
            // We will check the rest of the instructions after execution phase.
            Bytecode::Pop
            | Bytecode::LdU8(_)
            | Bytecode::LdU16(_)
            | Bytecode::LdU32(_)
            | Bytecode::LdU64(_)
            | Bytecode::LdU128(_)
            | Bytecode::LdU256(_)
            | Bytecode::LdTrue
            | Bytecode::LdFalse
            | Bytecode::LdConst(_)
            | Bytecode::CopyLoc(_)
            | Bytecode::MoveLoc(_)
            | Bytecode::MutBorrowLoc(_)
            | Bytecode::ImmBorrowLoc(_)
            | Bytecode::ImmBorrowField(_)
            | Bytecode::MutBorrowField(_)
            | Bytecode::ImmBorrowFieldGeneric(_)
            | Bytecode::MutBorrowFieldGeneric(_)
            | Bytecode::Pack(_)
            | Bytecode::PackGeneric(_)
            | Bytecode::Unpack(_)
            | Bytecode::UnpackGeneric(_)
            | Bytecode::ReadRef
            | Bytecode::WriteRef
            | Bytecode::CastU8
            | Bytecode::CastU16
            | Bytecode::CastU32
            | Bytecode::CastU64
            | Bytecode::CastU128
            | Bytecode::CastU256
            | Bytecode::Add
            | Bytecode::Sub
            | Bytecode::Mul
            | Bytecode::Mod
            | Bytecode::Div
            | Bytecode::BitOr
            | Bytecode::BitAnd
            | Bytecode::Xor
            | Bytecode::Or
            | Bytecode::And
            | Bytecode::Shl
            | Bytecode::Shr
            | Bytecode::Lt
            | Bytecode::Le
            | Bytecode::Gt
            | Bytecode::Ge
            | Bytecode::Eq
            | Bytecode::Neq
            | Bytecode::MutBorrowGlobal(_)
            | Bytecode::ImmBorrowGlobal(_)
            | Bytecode::MutBorrowGlobalGeneric(_)
            | Bytecode::ImmBorrowGlobalGeneric(_)
            | Bytecode::Exists(_)
            | Bytecode::ExistsGeneric(_)
            | Bytecode::MoveTo(_)
            | Bytecode::MoveToGeneric(_)
            | Bytecode::MoveFrom(_)
            | Bytecode::MoveFromGeneric(_)
            | Bytecode::FreezeRef
            | Bytecode::Nop
            | Bytecode::Not
            | Bytecode::VecPack(_, _)
            | Bytecode::VecLen(_)
            | Bytecode::VecImmBorrow(_)
            | Bytecode::VecMutBorrow(_)
            | Bytecode::VecPushBack(_)
            | Bytecode::VecPopBack(_)
            | Bytecode::VecUnpack(_, _)
            | Bytecode::VecSwap(_) => (),
        };
        Ok(())
    }

    /// Paranoid type checks to perform after instruction execution.
    ///
    /// This function and `pre_execution_type_stack_transition` should constitute the full type stack transition for the paranoid mode.
    fn post_execution_type_stack_transition(
        &mut self,
        local_tys: &[Type],
        ty_args: &[Type],
        resolver: &Resolver,
        instruction: &Bytecode,
    ) -> PartialVMResult<()> {
        match instruction {
            Bytecode::BrTrue(_) | Bytecode::BrFalse(_) => (),
            Bytecode::Branch(_)
            | Bytecode::Ret
            | Bytecode::Call(_)
            | Bytecode::CallGeneric(_)
            | Bytecode::Abort => {
                // Invariants hold because all of the instructions above will force VM to break from the interpreter loop and thus not hit this code path.
                unreachable!("control flow instruction encountered during type check")
            }
            Bytecode::Pop => {
                let ty = self.type_stack.pop_ty()?;
                check_ability(resolver.loader().abilities(&ty)?.has_drop())?;
            }
            Bytecode::LdU8(_) => self.type_stack.push_ty(Type::U8),
            Bytecode::LdU16(_) => self.type_stack.push_ty(Type::U16),
            Bytecode::LdU32(_) => self.type_stack.push_ty(Type::U32),
            Bytecode::LdU64(_) => self.type_stack.push_ty(Type::U64),
            Bytecode::LdU128(_) => self.type_stack.push_ty(Type::U128),
            Bytecode::LdU256(_) => self.type_stack.push_ty(Type::U256),
            Bytecode::LdTrue | Bytecode::LdFalse => self.type_stack.push_ty(Type::Bool),
            Bytecode::LdConst(i) => {
                let constant = resolver.constant_at(*i);
                self.type_stack
                    .push_ty(Type::from_const_signature(&constant.type_)?);
            }
            Bytecode::CopyLoc(idx) => {
                let ty = local_tys[*idx as usize].clone();
                check_ability(resolver.loader().abilities(&ty)?.has_copy())?;
                self.type_stack.push_ty(ty);
            }
            Bytecode::MoveLoc(idx) => {
                let ty = local_tys[*idx as usize].clone();
                self.type_stack.push_ty(ty);
            }
            Bytecode::StLoc(_) => (),
            Bytecode::MutBorrowLoc(idx) => {
                let ty = local_tys[*idx as usize].clone();
                self.type_stack
                    .push_ty(Type::MutableReference(Box::new(ty)));
            }
            Bytecode::ImmBorrowLoc(idx) => {
                let ty = local_tys[*idx as usize].clone();
                self.type_stack.push_ty(Type::Reference(Box::new(ty)));
            }
            Bytecode::ImmBorrowField(fh_idx) => {
                let expected_ty = resolver.field_handle_to_struct(*fh_idx);
                let top_ty = self.type_stack.pop_ty()?;
                top_ty.check_ref_eq(&expected_ty)?;
                self.type_stack
                    .push_ty(Type::Reference(Box::new(resolver.get_field_type(*fh_idx)?)));
            }
            Bytecode::MutBorrowField(fh_idx) => {
                let expected_ty = resolver.field_handle_to_struct(*fh_idx);
                let top_ty = self.type_stack.pop_ty()?;
                top_ty.check_eq(&Type::MutableReference(Box::new(expected_ty)))?;
                self.type_stack.push_ty(Type::MutableReference(Box::new(
                    resolver.get_field_type(*fh_idx)?,
                )));
            }
            Bytecode::ImmBorrowFieldGeneric(idx) => {
                let expected_ty = resolver.field_instantiation_to_struct(*idx, ty_args)?;
                let top_ty = self.type_stack.pop_ty()?;
                top_ty.check_ref_eq(&expected_ty)?;
                self.type_stack.push_ty(Type::Reference(Box::new(
                    resolver.instantiate_generic_field(*idx, ty_args)?,
                )));
            }
            Bytecode::MutBorrowFieldGeneric(idx) => {
                let expected_ty = resolver.field_instantiation_to_struct(*idx, ty_args)?;
                let top_ty = self.type_stack.pop_ty()?;
                top_ty.check_eq(&Type::MutableReference(Box::new(expected_ty)))?;
                self.type_stack.push_ty(Type::MutableReference(Box::new(
                    resolver.instantiate_generic_field(*idx, ty_args)?,
                )));
            }
            Bytecode::Pack(idx) => {
                let field_count = resolver.field_count(*idx);
                let args_ty = resolver.get_struct_fields(*idx)?;
                let output_ty = resolver.get_struct_type(*idx);
                let ability = resolver.loader().abilities(&output_ty)?;

                // If the struct has a key ability, we expects all of its field to have store ability but not key ability.
                let field_expected_abilities = if ability.has_key() {
                    ability
                        .remove(Ability::Key)
                        .union(AbilitySet::singleton(Ability::Store))
                } else {
                    ability
                };

                if field_count as usize != args_ty.fields.len() {
                    return Err(
                        PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                            .with_message("Args count mismatch".to_string()),
                    );
                }

                for (ty, expected_ty) in self
                    .type_stack
                    .popn_tys(field_count)?
                    .into_iter()
                    .zip(args_ty.fields.iter())
                {
                    // Fields ability should be a subset of the struct ability because abilities can be weakened but not the other direction.
                    // For example, it is ok to have a struct that doesn't have a copy capability where its field is a struct that has copy capability but not vice versa.
                    check_ability(
                        field_expected_abilities.is_subset(resolver.loader().abilities(&ty)?),
                    )?;
                    ty.check_eq(expected_ty)?;
                }

                self.type_stack.push_ty(output_ty);
            }
            Bytecode::PackGeneric(idx) => {
                let field_count = resolver.field_instantiation_count(*idx);
                let args_ty = resolver.instantiate_generic_struct_fields(*idx, ty_args)?;
                let output_ty = resolver.instantiate_generic_type(*idx, ty_args)?;
                let ability = resolver.loader().abilities(&output_ty)?;

                // If the struct has a key ability, we expects all of its field to have store ability but not key ability.
                let field_expected_abilities = if ability.has_key() {
                    ability
                        .remove(Ability::Key)
                        .union(AbilitySet::singleton(Ability::Store))
                } else {
                    ability
                };

                if field_count as usize != args_ty.len() {
                    return Err(
                        PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                            .with_message("Args count mismatch".to_string()),
                    );
                }

                for (ty, expected_ty) in self
                    .type_stack
                    .popn_tys(field_count)?
                    .into_iter()
                    .zip(args_ty.iter())
                {
                    // Fields ability should be a subset of the struct ability because abilities can be weakened but not the other direction.
                    // For example, it is ok to have a struct that doesn't have a copy capability where its field is a struct that has copy capability but not vice versa.
                    check_ability(
                        field_expected_abilities.is_subset(resolver.loader().abilities(&ty)?),
                    )?;
                    ty.check_eq(expected_ty)?;
                }

                self.type_stack.push_ty(output_ty)
            }
            Bytecode::Unpack(idx) => {
                let struct_ty = self.type_stack.pop_ty()?;
                struct_ty.check_eq(&resolver.get_struct_type(*idx))?;
                let struct_decl = resolver.get_struct_fields(*idx)?;
                for ty in struct_decl.fields.iter() {
                    self.type_stack.push_ty(ty.clone());
                }
            }
            Bytecode::UnpackGeneric(idx) => {
                let struct_ty = self.type_stack.pop_ty()?;
                struct_ty.check_eq(&resolver.instantiate_generic_type(*idx, ty_args)?)?;

                let struct_decl = resolver.instantiate_generic_struct_fields(*idx, ty_args)?;
                for ty in struct_decl.into_iter() {
                    self.type_stack.push_ty(ty.clone());
                }
            }
            Bytecode::ReadRef => {
                let ref_ty = self.type_stack.pop_ty()?;
                match ref_ty {
                    Type::Reference(inner) | Type::MutableReference(inner) => {
                        check_ability(resolver.loader().abilities(&inner)?.has_copy())?;
                        self.type_stack.push_ty(inner.as_ref().clone());
                    }
                    _ => {
                        return Err(PartialVMError::new(
                            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
                        )
                        .with_message("ReadRef expecting a value of reference type".to_string()))
                    }
                }
            }
            Bytecode::WriteRef => {
                let ref_ty = self.type_stack.pop_ty()?;
                let val_ty = self.type_stack.pop_ty()?;
                match ref_ty {
                    Type::MutableReference(inner) => {
                        if *inner == val_ty {
                            check_ability(resolver.loader().abilities(&inner)?.has_drop())?;
                        } else {
                            return Err(PartialVMError::new(
                                StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
                            )
                            .with_message(
                                "WriteRef tried to write references of different types".to_string(),
                            ));
                        }
                    }
                    _ => {
                        return Err(PartialVMError::new(
                            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
                        )
                        .with_message(
                            "WriteRef expecting a value of mutable reference type".to_string(),
                        ))
                    }
                }
            }
            Bytecode::CastU8 => {
                self.type_stack.pop_ty()?;
                self.type_stack.push_ty(Type::U8);
            }
            Bytecode::CastU16 => {
                self.type_stack.pop_ty()?;
                self.type_stack.push_ty(Type::U16);
            }
            Bytecode::CastU32 => {
                self.type_stack.pop_ty()?;
                self.type_stack.push_ty(Type::U32);
            }
            Bytecode::CastU64 => {
                self.type_stack.pop_ty()?;
                self.type_stack.push_ty(Type::U64);
            }
            Bytecode::CastU128 => {
                self.type_stack.pop_ty()?;
                self.type_stack.push_ty(Type::U128);
            }
            Bytecode::CastU256 => {
                self.type_stack.pop_ty()?;
                self.type_stack.push_ty(Type::U256);
            }
            Bytecode::Add
            | Bytecode::Sub
            | Bytecode::Mul
            | Bytecode::Mod
            | Bytecode::Div
            | Bytecode::BitOr
            | Bytecode::BitAnd
            | Bytecode::Xor
            | Bytecode::Or
            | Bytecode::And => {
                let lhs = self.type_stack.pop_ty()?;
                let rhs = self.type_stack.pop_ty()?;
                lhs.check_eq(&rhs)?;
                self.type_stack.push_ty(lhs);
            }
            Bytecode::Shl | Bytecode::Shr => {
                self.type_stack.pop_ty()?;
                let rhs = self.type_stack.pop_ty()?;
                self.type_stack.push_ty(rhs);
            }
            Bytecode::Lt | Bytecode::Le | Bytecode::Gt | Bytecode::Ge => {
                let lhs = self.type_stack.pop_ty()?;
                let rhs = self.type_stack.pop_ty()?;
                lhs.check_eq(&rhs)?;
                self.type_stack.push_ty(Type::Bool);
            }
            Bytecode::Eq | Bytecode::Neq => {
                let lhs = self.type_stack.pop_ty()?;
                let rhs = self.type_stack.pop_ty()?;
                if lhs != rhs {
                    return Err(
                        PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                            .with_message(
                                "Integer binary operation expecting values of same type"
                                    .to_string(),
                            ),
                    );
                }
                check_ability(resolver.loader().abilities(&lhs)?.has_drop())?;
                self.type_stack.push_ty(Type::Bool);
            }
            Bytecode::MutBorrowGlobal(idx) => {
                self.type_stack.pop_ty()?.check_eq(&Type::Address)?;
                let ty = resolver.get_struct_type(*idx);
                check_ability(resolver.loader().abilities(&ty)?.has_key())?;
                self.type_stack
                    .push_ty(Type::MutableReference(Box::new(ty)));
            }
            Bytecode::ImmBorrowGlobal(idx) => {
                self.type_stack.pop_ty()?.check_eq(&Type::Address)?;
                let ty = resolver.get_struct_type(*idx);
                check_ability(resolver.loader().abilities(&ty)?.has_key())?;
                self.type_stack.push_ty(Type::Reference(Box::new(ty)));
            }
            Bytecode::MutBorrowGlobalGeneric(idx) => {
                self.type_stack.pop_ty()?.check_eq(&Type::Address)?;
                let ty = resolver.instantiate_generic_type(*idx, ty_args)?;
                check_ability(resolver.loader().abilities(&ty)?.has_key())?;
                self.type_stack
                    .push_ty(Type::MutableReference(Box::new(ty)));
            }
            Bytecode::ImmBorrowGlobalGeneric(idx) => {
                self.type_stack.pop_ty()?.check_eq(&Type::Address)?;
                let ty = resolver.instantiate_generic_type(*idx, ty_args)?;
                check_ability(resolver.loader().abilities(&ty)?.has_key())?;
                self.type_stack.push_ty(Type::Reference(Box::new(ty)));
            }
            Bytecode::Exists(_) | Bytecode::ExistsGeneric(_) => {
                self.type_stack.pop_ty()?.check_eq(&Type::Address)?;
                self.type_stack.push_ty(Type::Bool);
            }
            Bytecode::MoveTo(idx) => {
                let ty = self.type_stack.pop_ty()?;
                self.type_stack
                    .pop_ty()?
                    .check_eq(&Type::Reference(Box::new(Type::Signer)))?;
                ty.check_eq(&resolver.get_struct_type(*idx))?;
                check_ability(resolver.loader().abilities(&ty)?.has_key())?;
            }
            Bytecode::MoveToGeneric(idx) => {
                let ty = self.type_stack.pop_ty()?;
                self.type_stack
                    .pop_ty()?
                    .check_eq(&Type::Reference(Box::new(Type::Signer)))?;
                ty.check_eq(&resolver.instantiate_generic_type(*idx, ty_args)?)?;
                check_ability(resolver.loader().abilities(&ty)?.has_key())?;
            }
            Bytecode::MoveFrom(idx) => {
                self.type_stack.pop_ty()?.check_eq(&Type::Address)?;
                let ty = resolver.get_struct_type(*idx);
                check_ability(resolver.loader().abilities(&ty)?.has_key())?;
                self.type_stack.push_ty(ty);
            }
            Bytecode::MoveFromGeneric(idx) => {
                self.type_stack.pop_ty()?.check_eq(&Type::Address)?;
                let ty = resolver.instantiate_generic_type(*idx, ty_args)?;
                check_ability(resolver.loader().abilities(&ty)?.has_key())?;
                self.type_stack.push_ty(ty);
            }
            Bytecode::FreezeRef => {
                match self.type_stack.pop_ty()? {
                    Type::MutableReference(ty) => self.type_stack.push_ty(Type::Reference(ty)),
                    _ => {
                        return Err(PartialVMError::new(
                            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
                        )
                        .with_message("FreezeRef expects a mutable reference".to_string()))
                    }
                };
            }
            Bytecode::Nop => (),
            Bytecode::Not => {
                self.type_stack.pop_ty()?.check_eq(&Type::Bool)?;
                self.type_stack.push_ty(Type::Bool);
            }
            Bytecode::VecPack(si, num) => {
                let ty = resolver.instantiate_single_type(*si, ty_args)?;
                let elem_tys = self.type_stack.popn_tys(*num as u16)?;
                for elem_ty in elem_tys.iter() {
                    elem_ty.check_eq(&ty)?;
                }
                self.type_stack.push_ty(Type::Vector(Box::new(ty)));
            }
            Bytecode::VecLen(si) => {
                let ty = resolver.instantiate_single_type(*si, ty_args)?;
                self.type_stack.pop_ty()?.check_vec_ref(&ty, false)?;
                self.type_stack.push_ty(Type::U64);
            }
            Bytecode::VecImmBorrow(si) => {
                let ty = resolver.instantiate_single_type(*si, ty_args)?;
                self.type_stack.pop_ty()?.check_eq(&Type::U64)?;
                let inner_ty = self.type_stack.pop_ty()?.check_vec_ref(&ty, false)?;
                self.type_stack.push_ty(Type::Reference(Box::new(inner_ty)));
            }
            Bytecode::VecMutBorrow(si) => {
                let ty = resolver.instantiate_single_type(*si, ty_args)?;
                self.type_stack.pop_ty()?.check_eq(&Type::U64)?;
                let inner_ty = self.type_stack.pop_ty()?.check_vec_ref(&ty, true)?;
                self.type_stack
                    .push_ty(Type::MutableReference(Box::new(inner_ty)));
            }
            Bytecode::VecPushBack(si) => {
                let ty = resolver.instantiate_single_type(*si, ty_args)?;
                self.type_stack.pop_ty()?.check_eq(&ty)?;
                self.type_stack.pop_ty()?.check_vec_ref(&ty, true)?;
            }
            Bytecode::VecPopBack(si) => {
                let ty = resolver.instantiate_single_type(*si, ty_args)?;
                let inner_ty = self.type_stack.pop_ty()?.check_vec_ref(&ty, true)?;
                self.type_stack.push_ty(inner_ty);
            }
            Bytecode::VecUnpack(si, num) => {
                let ty = resolver.instantiate_single_type(*si, ty_args)?;
                let vec_ty = self.type_stack.pop_ty()?;
                match vec_ty {
                    Type::Vector(v) => {
                        v.check_eq(&ty)?;
                        for _ in 0..*num {
                            self.type_stack.push_ty(v.as_ref().clone());
                        }
                    }
                    _ => {
                        return Err(PartialVMError::new(
                            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
                        )
                        .with_message("VecUnpack expect a vector type".to_string()))
                    }
                };
            }
            Bytecode::VecSwap(si) => {
                let ty = resolver.instantiate_single_type(*si, ty_args)?;
                self.type_stack.pop_ty()?.check_eq(&Type::U64)?;
                self.type_stack.pop_ty()?.check_eq(&Type::U64)?;
                self.type_stack.pop_ty()?.check_vec_ref(&ty, true)?;
            }
        }
        Ok(())
    }
}
