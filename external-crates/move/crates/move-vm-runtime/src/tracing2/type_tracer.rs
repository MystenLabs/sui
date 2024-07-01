use std::{cell::RefCell, collections::BTreeMap};

use move_binary_format::file_format::ConstantPoolIndex;
use move_core_types::{
    account_address::AccountAddress,
    annotated_value::{MoveStruct, MoveTypeLayout, MoveValue, MoveVariant},
    language_storage::TypeTag,
};
use move_trace_format::trace_format::{
    FrameIdentifier, InstructionEffect as IF, Location, MoveTrace, Read, RefType, TraceValue,
    TypeTagWithRefs, Write,
};
use move_vm_types::{
    loaded_data::runtime_types::Type,
    values::{IntegerValue, Value},
};

use crate::{
    interpreter::Frame,
    loader::{Function, Loader},
};

use super::trace_interface::Tracer;

pub struct TypeTracer {
    inner: RefCell<InnerTypeTracer>,
}

impl TypeTracer {
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(InnerTypeTracer::new()),
        }
    }
}

pub struct InnerTypeTracer {
    frame_counter: usize,
    current_stack: Vec<TraceValue>,
    active_frames: BTreeMap<FrameIdentifier, BTreeMap<usize, TraceValue>>,
    trace: MoveTrace,
    link_context: Option<AccountAddress>,
}

impl InnerTypeTracer {
    fn new() -> Self {
        Self {
            frame_counter: 0,
            current_stack: vec![],
            active_frames: BTreeMap::new(),
            trace: MoveTrace::new(),
            link_context: None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum WriteType {
    Put(MoveValue),
    Push(MoveValue),
    Pop,
    Swap(usize, usize),
}

pub enum WriteResult {
    Put(TraceValue),
    Pop {
        popped: TraceValue,
        result: TraceValue,
    },
}

impl Tracer for TypeTracer {
    fn name(&self) -> String {
        self.inner.borrow().name().to_owned()
    }

    fn open_main_frame(
        &self,
        args: &[Value],
        ty_args: &[Type],
        function: &Function,
        loader: &Loader,
        remaining_gas: u64,
        link_context: AccountAddress,
    ) {
        self.inner.borrow_mut().open_main_frame(
            args,
            ty_args,
            function,
            loader,
            remaining_gas,
            link_context,
        )
    }

    fn close_main_frame(
        &self,
        ty_args: &[Type],
        return_values: &[Value],
        function: &Function,
        loader: &Loader,
        remaining_gas: u64,
        link_context: AccountAddress,
    ) {
        self.inner.borrow_mut().close_main_frame(
            ty_args,
            return_values,
            function,
            loader,
            remaining_gas,
            link_context,
        )
    }

    fn open_frame(
        &self,
        ty_args: &[Type],
        function: &Function,
        loader: &Loader,
        remaining_gas: u64,
        link_context: AccountAddress,
    ) {
        self.inner
            .borrow_mut()
            .open_frame(ty_args, function, loader, remaining_gas, link_context)
    }

    fn close_frame(
        &self,
        function: &Function,
        loader: &Loader,
        remaining_gas: u64,
        link_context: AccountAddress,
    ) {
        self.inner
            .borrow_mut()
            .close_frame(function, loader, remaining_gas, link_context)
    }

    fn open_instruction(&self, frame: &Frame, loader: &Loader, remaining_gas: u64) {
        self.inner
            .borrow_mut()
            .open_instruction(frame, loader, remaining_gas)
    }

    fn close_instruction(&self, pc: u16, function: &Function, loader: &Loader, remaining_gas: u64) {
        self.inner
            .borrow_mut()
            .close_instruction(pc, function, loader, remaining_gas)
    }
}

impl InnerTypeTracer {
    fn increment_frame_counter(&mut self) -> usize {
        let counter = self.frame_counter;
        self.frame_counter += 1;
        counter
    }

    fn current_frame(&self) -> FrameIdentifier {
        *self.active_frames.last_key_value().unwrap().0
    }

    fn insert_local(&mut self, local_index: usize, local: TraceValue) {
        self.active_frames
            .last_entry()
            .unwrap()
            .get_mut()
            .insert(local_index, local);
    }

    fn get_local(&self, frame_index: FrameIdentifier, local_index: usize) -> TraceValue {
        self.active_frames
            .get(&frame_index)
            .unwrap()
            .get(&local_index)
            .unwrap()
            .clone()
    }

    fn get_local_mut(
        &mut self,
        frame_index: FrameIdentifier,
        local_index: usize,
    ) -> &mut TraceValue {
        self.active_frames
            .get_mut(&frame_index)
            .unwrap()
            .get_mut(&local_index)
            .unwrap()
    }

    fn copy_root_value(&self, loc: &Location) -> TraceValue {
        match loc {
            Location::Local(fidx, loc_idx) => self.get_local(*fidx, *loc_idx),
            Location::Stack(stack_idx) => self.current_stack[*stack_idx as usize].clone(),
            Location::Indexed(loc, idx) => {
                let val = match &**loc {
                    // Currnetly only support one layer
                    Location::Indexed(loc, idx) => {
                        let val = self.copy_root_value(loc);
                        match val.value().unwrap() {
                            MoveValue::Struct(s) => {
                                let field = s.fields.get(*idx as usize).unwrap().1.clone();
                                TraceValue::Value { value: field }
                            }
                            MoveValue::Vector(v) => {
                                let field = v.get(*idx as usize).unwrap().clone();
                                TraceValue::Value { value: field }
                            }
                            _ => {
                                panic!("Expected struct or vector, got {:?}", val.value().unwrap())
                            }
                        }
                    }
                    Location::Local(..) | Location::Stack(_) => self.copy_root_value(loc),
                };
                match val.value().unwrap() {
                    MoveValue::Struct(s) => {
                        let field = s.fields.get(*idx as usize).unwrap().1.clone();
                        TraceValue::Value { value: field }
                    }
                    MoveValue::Vector(v) => {
                        let field = v.get(*idx as usize).unwrap().clone();
                        TraceValue::Value { value: field }
                    }
                    _ => panic!("Expected struct or vector, got {:?}", val.value().unwrap()),
                }
            }
        }
    }

    fn mut_root_value(&mut self, loc: &Location) -> &mut MoveValue {
        match loc {
            Location::Local(fidx, loc_idx) => {
                self.get_local_mut(*fidx, *loc_idx).value_mut().unwrap()
            }
            Location::Stack(stack_idx) => {
                self.current_stack[*stack_idx as usize].value_mut().unwrap()
            }
            Location::Indexed(loc, idx) => {
                let val = match &**loc {
                    // Currnetly only support one layer
                    Location::Indexed(loc, idx) => {
                        let val = self.mut_root_value(loc);
                        match val {
                            MoveValue::Struct(s) => &mut s.fields.get_mut(*idx as usize).unwrap().1,
                            MoveValue::Vector(v) => v.get_mut(*idx as usize).unwrap(),
                            _ => {
                                panic!("Expected struct or vector, got {:#}", val)
                            }
                        }
                    }
                    Location::Local(..) | Location::Stack(_) => self.mut_root_value(loc),
                };
                match val {
                    MoveValue::Struct(s) => &mut s.fields.get_mut(*idx as usize).unwrap().1,
                    MoveValue::Vector(v) => v.get_mut(*idx as usize).unwrap(),
                    _ => panic!("Expected struct or vector, got {:#}", val),
                }
            }
        }
    }

    fn write_root_value(&mut self, loc: &Location, write: WriteType) -> WriteResult {
        let val = self.mut_root_value(loc);
        match write {
            WriteType::Put(v) => {
                *val = v;
                WriteResult::Put(TraceValue::Value { value: val.clone() })
            }
            WriteType::Push(v) => {
                match val {
                    MoveValue::Vector(vs) => {
                        vs.push(v);
                    }
                    _ => panic!("Expected vector for Push, got {:#}", val),
                }
                WriteResult::Put(TraceValue::Value { value: val.clone() })
            }
            WriteType::Pop => {
                let popped = match val {
                    MoveValue::Vector(vs) => vs.pop().unwrap(),
                    _ => panic!("Expected vector for Pop, got {:#}", val),
                };
                WriteResult::Pop {
                    popped: TraceValue::Value { value: popped },
                    result: TraceValue::Value { value: val.clone() },
                }
            }
            WriteType::Swap(a, b) => {
                match val {
                    MoveValue::Vector(vs) => {
                        vs.swap(a, b);
                    }
                    _ => panic!("Expected vector for Swap, got {:#}", val),
                }
                WriteResult::Put(TraceValue::Value { value: val.clone() })
            }
        }
    }
}

impl InnerTypeTracer {
    fn name(&self) -> &str {
        "TypeTracer"
    }

    fn open_main_frame(
        &mut self,
        args: &[Value],
        ty_args: &[Type],
        function: &Function,
        loader: &Loader,
        remaining_gas: u64,
        link_context: AccountAddress,
    ) {
        self.link_context = Some(link_context);

        let function_type_info = get_function_types(function, loader, ty_args, link_context);
        let frame_idx = self.increment_frame_counter();

        let call_args: Vec<_> = args
            .iter()
            .zip(function_type_info.local_types.iter().cloned())
            .map(|(value, (layout, ref_type))| {
                let move_value = value.as_move_value(&layout.undecorate()).decorate(&layout);
                assert!(ref_type.is_none());
                TraceValue::Value { value: move_value }
            })
            .collect();
        let starting_locals = call_args
            .iter()
            .enumerate()
            .map(|(i, v)| (i, v.clone()))
            .collect();

        self.active_frames
            .entry(frame_idx)
            .or_insert(starting_locals);

        self.trace.open_frame(
            frame_idx,
            function.index(),
            function.name().to_string(),
            function.module_id().clone(),
            call_args,
            function_type_info.ty_args,
            function_type_info.return_types,
            function_type_info
                .local_types
                .into_iter()
                .map(|(layout, ref_type)| TypeTagWithRefs {
                    layout: (&layout).into(),
                    ref_type,
                })
                .collect(),
            remaining_gas,
        );
    }

    fn close_main_frame(
        &mut self,
        ty_args: &[Type],
        return_values: &[Value],
        function: &Function,
        loader: &Loader,
        remaining_gas: u64,
        link_context: AccountAddress,
    ) {
        let function_type_info = get_function_types(function, loader, ty_args, link_context);
        let frame_idx = self.active_frames.pop_last().unwrap().0;
        let return_values: Vec<_> = return_values
            .iter()
            .zip(function_type_info.local_types.iter().cloned())
            .map(|(value, (layout, ref_type))| {
                let move_value = value.as_move_value(&layout.undecorate()).decorate(&layout);
                assert!(ref_type.is_none());
                TraceValue::Value { value: move_value }
            })
            .collect();
        self.trace
            .close_frame(frame_idx, return_values, remaining_gas);
    }

    fn open_frame(
        &mut self,
        ty_args: &[Type],
        function: &Function,
        loader: &Loader,
        remaining_gas: u64,
        link_context: AccountAddress,
    ) {
        self.link_context = Some(link_context);

        let len = self.current_stack.len();
        let call_args: Vec<_> = self.current_stack.split_off(len - function.arg_count());
        let function_type_info = get_function_types(function, loader, ty_args, link_context);
        let starting_locals = call_args
            .iter()
            .enumerate()
            .map(|(i, v)| (i, v.clone()))
            .collect();
        let frame_idx = self.increment_frame_counter();
        self.active_frames
            .entry(frame_idx)
            .or_insert(starting_locals);

        self.trace.open_frame(
            frame_idx,
            function.index(),
            function.name().to_string(),
            function.module_id().clone(),
            call_args,
            function_type_info.ty_args,
            function_type_info.return_types,
            function_type_info
                .local_types
                .into_iter()
                .map(|(layout, ref_type)| TypeTagWithRefs {
                    layout: (&layout).into(),
                    ref_type,
                })
                .collect(),
            remaining_gas,
        );
    }

    fn close_frame(
        &mut self,
        function: &Function,
        _loader: &Loader,
        remaining_gas: u64,
        _link_context: AccountAddress,
    ) {
        let len = self.current_stack.len();
        let return_values = self.current_stack[(len - function.return_type_count())..].to_vec();
        let frame_idx = self.active_frames.pop_last().unwrap().0;
        self.trace
            .close_frame(frame_idx, return_values, remaining_gas);
    }

    fn open_instruction(&mut self, frame: &Frame, loader: &Loader, remaining_gas: u64) {
        use move_binary_format::file_format::Bytecode as B;
        let pc = frame.pc;
        let current_frame = self.current_frame();
        match &frame.function.code()[pc as usize] {
            B::Pop | B::BrTrue(_) | B::BrFalse(_) => {
                let v = self.current_stack.pop().unwrap();
                let effects = vec![IF::Pop(v)];
                self.trace.instruction(vec![], effects, remaining_gas, pc);
            }
            B::Branch(_) | B::Ret => {
                let effects = vec![];
                self.trace.instruction(vec![], effects, remaining_gas, pc);
            }
            i @ (B::LdU8(_)
            | B::LdU16(_)
            | B::LdU32(_)
            | B::LdU64(_)
            | B::LdU128(_)
            | B::LdU256(_)
            | B::LdFalse
            | B::LdTrue
            | B::LdConst(_)) => {
                let move_value = match i {
                    B::LdU8(u) => MoveValue::U8(*u),
                    B::LdU16(u) => MoveValue::U16(*u),
                    B::LdU32(u) => MoveValue::U32(*u),
                    B::LdU64(u) => MoveValue::U64(*u),
                    B::LdU128(u) => MoveValue::U128(**u),
                    B::LdU256(u) => MoveValue::U256(**u),
                    B::LdTrue => MoveValue::Bool(true),
                    B::LdFalse => MoveValue::Bool(false),
                    B::LdConst(const_idx) => get_constant(
                        &frame.function,
                        loader,
                        self.link_context.unwrap(),
                        *const_idx,
                    ),
                    _ => unreachable!(),
                };
                let value = TraceValue::Value { value: move_value };
                let effects = vec![IF::Push(value.clone())];
                self.trace.instruction(vec![], effects, remaining_gas, pc);
                self.current_stack.push(value);
            }
            i @ (B::MoveLoc(l) | B::CopyLoc(l)) => {
                let v = self.get_local(current_frame, *l as usize);
                let effects = vec![
                    IF::Read(Read {
                        location: Location::Local(current_frame, *l as usize),
                        value_read: v.clone(),
                        moved: matches!(i, B::MoveLoc(_)),
                    }),
                    IF::Push(v.clone()),
                ];
                self.trace.instruction(vec![], effects, remaining_gas, pc);
                self.current_stack.push(v);
            }
            i @ (B::CastU8 | B::CastU16 | B::CastU32 | B::CastU64 | B::CastU128 | B::CastU256) => {
                let v = self.current_stack.pop().unwrap();
                let value = match i {
                    B::CastU8 => MoveValue::U8(
                        to_integer_value(&v.value().unwrap().clone())
                            .unwrap()
                            .cast_u8()
                            .unwrap(),
                    ),
                    B::CastU16 => MoveValue::U16(
                        to_integer_value(&v.value().unwrap().clone())
                            .unwrap()
                            .cast_u16()
                            .unwrap(),
                    ),
                    B::CastU32 => MoveValue::U32(
                        to_integer_value(&v.value().unwrap().clone())
                            .unwrap()
                            .cast_u32()
                            .unwrap(),
                    ),
                    B::CastU64 => MoveValue::U64(
                        to_integer_value(&v.value().unwrap().clone())
                            .unwrap()
                            .cast_u64()
                            .unwrap(),
                    ),

                    B::CastU128 => MoveValue::U128(
                        to_integer_value(&v.value().unwrap().clone())
                            .unwrap()
                            .cast_u128()
                            .unwrap(),
                    ),
                    B::CastU256 => MoveValue::U256(
                        to_integer_value(&v.value().unwrap().clone())
                            .unwrap()
                            .cast_u256()
                            .unwrap(),
                    ),
                    _ => unreachable!(),
                };
                let value = TraceValue::Value { value };
                let effects = vec![IF::Pop(v), IF::Push(value.clone())];
                self.trace.instruction(vec![], effects, remaining_gas, pc);
                self.current_stack.push(value)
            }
            B::StLoc(lidx) => {
                let v = self.current_stack.pop().unwrap();
                self.insert_local(*lidx as usize, v.clone());
                let effects = vec![
                    IF::Pop(v.clone()),
                    IF::Write(Write {
                        location: Location::Local(current_frame, *lidx as usize),
                        value_written: v.clone(),
                    }),
                ];
                self.trace.instruction(vec![], effects, remaining_gas, pc);
            }
            i @ (B::Add
            | B::Sub
            | B::Mul
            | B::Mod
            | B::Div
            | B::BitOr
            | B::BitAnd
            | B::Xor
            | B::Shl
            | B::Shr) => {
                let a = self.current_stack.pop().unwrap();
                let b = self.current_stack.pop().unwrap();
                let a_v = to_integer_value(&a.value().unwrap().clone()).unwrap();
                let b_v = to_integer_value(&b.value().unwrap().clone()).unwrap();
                let value = match i {
                    B::Add => a_v.add_checked(b_v).unwrap(),
                    B::Sub => a_v.sub_checked(b_v).unwrap(),
                    B::Mul => a_v.mul_checked(b_v).unwrap(),
                    B::Mod => a_v.rem_checked(b_v).unwrap(),
                    B::Div => a_v.div_checked(b_v).unwrap(),
                    B::BitOr => a_v.bit_or(b_v).unwrap(),
                    B::BitAnd => a_v.bit_and(b_v).unwrap(),
                    B::Xor => a_v.bit_xor(b_v).unwrap(),
                    B::Shl => {
                        let IntegerValue::U8(b_v) = b_v else {
                            panic!("Expected U8, got {:?}", b_v);
                        };
                        a_v.shl_checked(b_v).unwrap()
                    }
                    B::Shr => {
                        let IntegerValue::U8(b_v) = b_v else {
                            panic!("Expected U8, got {:?}", b_v);
                        };
                        a_v.shr_checked(b_v).unwrap()
                    }
                    _ => unreachable!(),
                };
                let value = from_integer_value(value);
                let value = TraceValue::Value { value };
                let effects = vec![
                    IF::Pop(a.clone()),
                    IF::Pop(b.clone()),
                    IF::Push(value.clone()),
                ];
                self.trace.instruction(vec![], effects, remaining_gas, pc);
                self.current_stack.push(value);
            }
            i @ (B::Lt | B::Gt | B::Le | B::Ge) => {
                let a = self.current_stack.pop().unwrap();
                let b = self.current_stack.pop().unwrap();
                let a_v = to_integer_value(&a.value().unwrap().clone()).unwrap();
                let b_v = to_integer_value(&b.value().unwrap().clone()).unwrap();
                let value = match i {
                    B::Lt => a_v.lt(b_v).unwrap(),
                    B::Gt => a_v.gt(b_v).unwrap(),
                    B::Le => a_v.le(b_v).unwrap(),
                    B::Ge => a_v.ge(b_v).unwrap(),
                    _ => unreachable!(),
                };
                let value = MoveValue::Bool(value);
                let value = TraceValue::Value { value };
                let effects = vec![
                    IF::Pop(a.clone()),
                    IF::Pop(b.clone()),
                    IF::Push(value.clone()),
                ];
                self.trace.instruction(vec![], effects, remaining_gas, pc);
                self.current_stack.push(value);
            }
            B::Call(_) | B::CallGeneric(_) => {
                // NB: We don't register effects for calls as they will be handled by the
                // open_frame.
                self.trace.instruction(vec![], vec![], remaining_gas, pc);
            }
            B::Pack(sidx) => {
                let resolver = frame
                    .function
                    .get_resolver(self.link_context.unwrap(), loader);
                let field_count = resolver.field_count(*sidx) as usize;
                let struct_type = resolver.get_struct_type(*sidx);
                let stack_len = self.current_stack.len();
                let fields = self.current_stack.split_off(stack_len - field_count);
                let ty = loader.type_to_fully_annotated_layout(&struct_type).unwrap();
                let MoveTypeLayout::Struct(slayout) = &ty else {
                    panic!("Expected struct, got {:?}", ty);
                };
                let mut effects = vec![];
                let mut struct_fields = vec![];
                assert!(
                    fields.len() == slayout.fields.len(),
                    "Popped fields and struct fields mismatch"
                );
                for (value, field_layout) in fields.iter().zip(slayout.fields.iter()) {
                    struct_fields.push((field_layout.name.clone(), value.value().unwrap().clone()));
                    effects.push(IF::Pop(value.clone()));
                }
                let value = TraceValue::Value {
                    value: MoveValue::Struct(MoveStruct {
                        type_: slayout.type_.clone(),
                        fields: struct_fields,
                    }),
                };
                effects.push(IF::Push(value.clone()));
                self.trace.instruction(vec![], effects, remaining_gas, pc);
                self.current_stack.push(value);
            }
            B::PackGeneric(sidx) => {
                let resolver = frame
                    .function
                    .get_resolver(self.link_context.unwrap(), loader);
                let field_count = resolver.field_instantiation_count(*sidx) as usize;
                let struct_type = resolver
                    .instantiate_struct_type(*sidx, &frame.ty_args)
                    .unwrap();
                let stack_len = self.current_stack.len();
                let fields = self.current_stack.split_off(stack_len - field_count);
                let ty = loader.type_to_fully_annotated_layout(&struct_type).unwrap();
                let MoveTypeLayout::Struct(slayout) = &ty else {
                    panic!("Expected struct, got {:?}", ty);
                };
                let mut effects = vec![];
                let mut struct_fields = vec![];
                assert!(
                    fields.len() == slayout.fields.len(),
                    "Popped fields and struct fields mismatch"
                );
                for (value, field_layout) in fields.iter().zip(slayout.fields.iter()) {
                    struct_fields.push((field_layout.name.clone(), value.value().unwrap().clone()));
                    effects.push(IF::Pop(value.clone()));
                }

                let value = TraceValue::Value {
                    value: MoveValue::Struct(MoveStruct {
                        type_: slayout.type_.clone(),
                        fields: struct_fields,
                    }),
                };

                effects.push(IF::Push(value.clone()));
                self.current_stack.push(value);
                // TODO: type args
                self.trace.instruction(vec![], effects, remaining_gas, pc);
            }
            B::Unpack(_) | B::UnpackGeneric(_) => {
                let value = self.current_stack.pop().unwrap();
                let MoveValue::Struct(s) = value.value().unwrap().clone() else {
                    panic!("Expected struct, got {:#}", value.value().unwrap());
                };

                let mut effects = vec![IF::Pop(value.clone())];

                for (_, value) in s.fields.iter() {
                    let value = TraceValue::Value {
                        value: value.clone(),
                    };
                    effects.push(IF::Push(value.clone()));
                    self.current_stack.push(value);
                }
                self.trace.instruction(vec![], effects, remaining_gas, pc);
            }

            i @ (B::Eq | B::Neq) => {
                let a_v = self.current_stack.pop().unwrap();
                let b_v = self.current_stack.pop().unwrap();
                let value = match i {
                    B::Eq => a_v.value().unwrap().eq(&b_v.value().unwrap()),
                    B::Neq => !a_v.value().unwrap().eq(&b_v.value().unwrap()),
                    _ => unreachable!(),
                };
                let value = TraceValue::Value {
                    value: MoveValue::Bool(value),
                };
                let effects = vec![
                    IF::Pop(a_v.clone()),
                    IF::Pop(b_v.clone()),
                    IF::Push(value.clone()),
                ];
                self.trace.instruction(vec![], effects, remaining_gas, pc);
                self.current_stack.push(value);
            }
            i @ (B::Or | B::And) => {
                let a = self.current_stack.pop().unwrap();
                let b = self.current_stack.pop().unwrap();
                let MoveValue::Bool(a_b) = a.value().unwrap().clone() else {
                    panic!("Expected bool, got {:?}", a.value().unwrap());
                };
                let MoveValue::Bool(b_b) = b.value().unwrap().clone() else {
                    panic!("Expected bool, got {:?}", b.value().unwrap());
                };
                let value = match i {
                    B::Or => a_b || b_b,
                    B::And => a_b && b_b,
                    _ => unreachable!(),
                };
                let value = MoveValue::Bool(value);
                let value = TraceValue::Value { value };
                let effects = vec![
                    IF::Pop(a.clone()),
                    IF::Pop(b.clone()),
                    IF::Push(value.clone()),
                ];
                self.trace.instruction(vec![], effects, remaining_gas, pc);
                self.current_stack.push(value);
            }
            B::Not => {
                let a = self.current_stack.pop().unwrap();
                let MoveValue::Bool(a_b) = a.value().unwrap().clone() else {
                    panic!("Expected bool, got {:?}", a.value().unwrap());
                };
                let value = !a_b;
                let value = MoveValue::Bool(value);
                let value = TraceValue::Value { value };
                let effects = vec![IF::Pop(a.clone()), IF::Push(value.clone())];
                self.trace.instruction(vec![], effects, remaining_gas, pc);
                self.current_stack.push(value);
            }

            B::Nop => {
                self.trace.instruction(vec![], vec![], remaining_gas, pc);
            }

            B::Abort => {
                let value = self.current_stack.pop().unwrap();
                let effects = vec![IF::Pop(value.clone())];
                self.trace.instruction(vec![], effects, remaining_gas, pc);
            }

            B::ReadRef => {
                let ref_value = self.current_stack.pop().unwrap();
                let location = ref_value.location().unwrap().clone();
                let value = self.copy_root_value(&location);

                let effects = vec![
                    IF::Pop(ref_value),
                    IF::Read(Read {
                        location,
                        value_read: value.clone(),
                        moved: false,
                    }),
                    IF::Push(value.clone()),
                ];
                self.trace.instruction(vec![], effects, remaining_gas, pc);
                self.current_stack.push(value);
            }
            B::ImmBorrowLoc(l_idx) => {
                let val = self.get_local(current_frame, *l_idx as usize);
                let location = Location::Local(current_frame, *l_idx as usize);
                let value = TraceValue::ImmRef {
                    location: location.clone(),
                    snapshot: Box::new(val.clone()),
                };
                let effects = vec![
                    IF::Read(Read {
                        location,
                        value_read: val,
                        moved: false,
                    }),
                    IF::Push(value.clone()),
                ];
                self.trace.instruction(vec![], effects, remaining_gas, pc);
                self.current_stack.push(value);
            }
            B::MutBorrowLoc(l_idx) => {
                let val = self.get_local(current_frame, *l_idx as usize);
                let location = Location::Local(current_frame, *l_idx as usize);
                let value = TraceValue::MutRef {
                    location: location.clone(),
                    snapshot: Box::new(val.clone()),
                };
                let effects = vec![
                    IF::Read(Read {
                        location,
                        value_read: val,
                        moved: false,
                    }),
                    IF::Push(value.clone()),
                ];
                self.trace.instruction(vec![], effects, remaining_gas, pc);
                self.current_stack.push(value);
            }
            B::WriteRef => {
                let reference = self.current_stack.pop().unwrap();
                let value_to_write = self.current_stack.pop().unwrap();
                let location = reference.location().unwrap().clone();
                let WriteResult::Put(written_value) = self.write_root_value(
                    &location,
                    WriteType::Put(value_to_write.value().unwrap().clone()),
                ) else {
                    panic!("Expected Put");
                };
                let effects = vec![
                    IF::Pop(reference.clone()),
                    IF::Pop(written_value.clone()),
                    IF::Write(Write {
                        location,
                        value_written: written_value.clone(),
                    }),
                ];
                self.trace.instruction(vec![], effects, remaining_gas, pc);
            }
            B::FreezeRef => {
                let reference = self.current_stack.pop().unwrap();
                let frozen_ref = match &reference {
                    TraceValue::MutRef { location, snapshot } => TraceValue::ImmRef {
                        location: location.clone(),
                        snapshot: snapshot.clone(),
                    },
                    _ => unreachable!("Expected reference, got {:?}", reference),
                };
                let effects = vec![IF::Pop(reference.clone()), IF::Push(frozen_ref.clone())];
                self.trace.instruction(vec![], effects, remaining_gas, pc);
                self.current_stack.push(frozen_ref);
            }

            i @ (B::MutBorrowField(fhidx) | B::ImmBorrowField(fhidx)) => {
                let value = self.current_stack.pop().unwrap();
                let location = value.location().unwrap();
                let resolver = frame
                    .function
                    .get_resolver(self.link_context.unwrap(), loader);
                let field_offset = resolver.field_offset(*fhidx);
                let field_ref = match i {
                    B::MutBorrowField(_) => TraceValue::MutRef {
                        location: Location::Indexed(Box::new(location.clone()), field_offset),
                        snapshot: Box::new(value.clone()),
                    },
                    B::ImmBorrowField(_) => TraceValue::ImmRef {
                        location: Location::Indexed(Box::new(location.clone()), field_offset),
                        snapshot: Box::new(value.clone()),
                    },
                    _ => unreachable!(),
                };
                let effects = vec![IF::Pop(value.clone()), IF::Push(field_ref.clone())];
                self.trace.instruction(vec![], effects, remaining_gas, pc);
                self.current_stack.push(field_ref);
            }
            i @ (B::MutBorrowFieldGeneric(fhidx) | B::ImmBorrowFieldGeneric(fhidx)) => {
                let value = self.current_stack.pop().unwrap();
                let location = value.location().unwrap();
                let resolver = frame
                    .function
                    .get_resolver(self.link_context.unwrap(), loader);
                let field_offset = resolver.field_instantiation_offset(*fhidx);
                let field_ref = match i {
                    B::MutBorrowFieldGeneric(_) => TraceValue::MutRef {
                        location: Location::Indexed(Box::new(location.clone()), field_offset),
                        snapshot: Box::new(value.clone()),
                    },
                    B::ImmBorrowFieldGeneric(_) => TraceValue::ImmRef {
                        location: Location::Indexed(Box::new(location.clone()), field_offset),
                        snapshot: Box::new(value.clone()),
                    },
                    _ => unreachable!(),
                };
                let effects = vec![IF::Pop(value.clone()), IF::Push(field_ref.clone())];
                // TODO: type args
                self.trace.instruction(vec![], effects, remaining_gas, pc);
                self.current_stack.push(field_ref);
            }
            B::VecPack(_, n) => {
                let stack_len = self.current_stack.len();
                let vals = self.current_stack.split_off(stack_len - *n as usize);
                let mut effects: Vec<_> = vals.iter().map(|v| IF::Pop(v.clone())).collect();
                let vector_value =
                    MoveValue::Vector(vals.iter().map(|v| v.value().unwrap().clone()).collect());
                let vector_value = TraceValue::Value {
                    value: vector_value,
                };
                effects.push(IF::Push(vector_value.clone()));
                self.trace.instruction(vec![], effects, remaining_gas, pc);
                self.current_stack.push(vector_value);
            }
            i @ (B::VecImmBorrow(_) | B::VecMutBorrow(_)) => {
                let index_val = self.current_stack.pop().unwrap();
                let reference_val = self.current_stack.pop().unwrap();
                let location = reference_val.location().unwrap();
                let MoveValue::U64(index_value) = index_val.value().unwrap() else {
                    panic!("Expected U64, got {:?}", index_val.value().unwrap());
                };
                let field_ref = match i {
                    B::VecMutBorrow(_) => TraceValue::MutRef {
                        location: Location::Indexed(
                            Box::new(location.clone()),
                            *index_value as usize,
                        ),
                        snapshot: Box::new(reference_val.clone()),
                    },
                    B::VecImmBorrow(_) => TraceValue::ImmRef {
                        location: Location::Indexed(
                            Box::new(location.clone()),
                            *index_value as usize,
                        ),
                        snapshot: Box::new(reference_val.clone()),
                    },
                    _ => unreachable!(),
                };
                let effects = vec![
                    IF::Pop(index_val.clone()),
                    IF::Pop(reference_val.clone()),
                    IF::Push(field_ref.clone()),
                ];
                self.trace.instruction(vec![], effects, remaining_gas, pc);
                self.current_stack.push(field_ref);
            }

            B::VecLen(_) => {
                let reference_val = self.current_stack.pop().unwrap();
                let location = reference_val.location().unwrap();
                let v = self.copy_root_value(&location).clone();
                let MoveValue::Vector(values) = v.value().unwrap() else {
                    panic!("Expected vector, got {:#}", v);
                };
                let len = values.len() as u64;
                let len_value = TraceValue::Value {
                    value: MoveValue::U64(len),
                };
                let effects = vec![IF::Pop(reference_val), IF::Push(len_value.clone())];
                self.trace.instruction(vec![], effects, remaining_gas, pc);
                self.current_stack.push(len_value);
            }
            B::VecPushBack(_) => {
                let value_vidx = self.current_stack.pop().unwrap();
                let reference_vidx = self.current_stack.pop().unwrap();
                let location = reference_vidx.location().unwrap().clone();
                let WriteResult::Put(written_value) = self.write_root_value(
                    &location,
                    WriteType::Push(value_vidx.value().unwrap().clone()),
                ) else {
                    panic!("Expected Put",);
                };
                let effects = vec![
                    IF::Pop(value_vidx),
                    IF::Pop(reference_vidx),
                    IF::Write(Write {
                        location,
                        value_written: written_value,
                    }),
                ];
                self.trace.instruction(vec![], effects, remaining_gas, pc);
            }
            B::VecPopBack(_) => {
                let reference_vidx = self.current_stack.pop().unwrap();
                let location = reference_vidx.location().unwrap().clone();
                let WriteResult::Pop { popped, result } =
                    self.write_root_value(&location, WriteType::Pop)
                else {
                    panic!("Expected Pop",);
                };
                let effects = vec![
                    IF::Pop(reference_vidx),
                    IF::Write(Write {
                        location,
                        value_written: result,
                    }),
                    IF::Push(popped.clone()),
                ];
                self.trace.instruction(vec![], effects, remaining_gas, pc);
                self.current_stack.push(popped);
            }
            B::VecUnpack(_, _) => {
                let value = self.current_stack.pop().unwrap();
                let MoveValue::Vector(values) = value.value().unwrap() else {
                    panic!("Expected vector, got {:?}", value);
                };
                let mut effects = vec![IF::Pop(value.clone())];
                let vals: Vec<_> = values
                    .into_iter()
                    .map(|v| TraceValue::Value { value: v.clone() })
                    .collect();
                effects.extend(vals.iter().cloned().map(IF::Push));
                self.trace.instruction(vec![], effects, remaining_gas, pc);
                self.current_stack.extend(vals);
            }
            B::VecSwap(_) => {
                let v1 = self.current_stack.pop().unwrap();
                let v2 = self.current_stack.pop().unwrap();
                let MoveValue::U64(v_idx1) = v1.value().unwrap() else {
                    panic!("Expected U64 but got {:?}", v1);
                };
                let MoveValue::U64(v_idx2) = v2.value().unwrap() else {
                    panic!("Expected U64 but got {:?}", v2);
                };
                let v_ref = self.current_stack.pop().unwrap();
                let location = v_ref.location().unwrap().clone();
                let WriteResult::Put(written_value) = self.write_root_value(
                    &location,
                    WriteType::Swap(*v_idx1 as usize, *v_idx2 as usize),
                ) else {
                    panic!("Expected Put");
                };
                let effects = vec![
                    IF::Pop(v1),
                    IF::Pop(v2),
                    IF::Pop(v_ref),
                    IF::Write(Write {
                        location,
                        value_written: written_value,
                    }),
                ];
                self.trace.instruction(vec![], effects, remaining_gas, pc);
            }
            B::PackVariant(vidx) => {
                let resolver = frame
                    .function
                    .get_resolver(self.link_context.unwrap(), loader);
                let (field_count, variant_tag) = resolver.variant_field_count_and_tag(*vidx);
                let stack_len = self.current_stack.len();
                let ty = loader
                    .type_to_fully_annotated_layout(&resolver.get_enum_type(*vidx))
                    .unwrap();
                let MoveTypeLayout::Enum(vlayouts) = &ty else {
                    panic!("Expected variant, got {:?}", ty);
                };
                let (name_tag, variant_layout) = vlayouts
                    .variants
                    .iter()
                    .find(|((_, tag), _)| *tag == variant_tag)
                    .unwrap()
                    .clone();
                let fields = self
                    .current_stack
                    .split_off(stack_len - field_count as usize);
                let mut effects = vec![];
                let mut variant_fields = vec![];
                assert!(
                    fields.len() == variant_layout.len(),
                    "Popped fields and variant fields mismatch"
                );
                for (value, field_layout) in fields.iter().zip(variant_layout.iter()) {
                    variant_fields
                        .push((field_layout.name.clone(), value.value().unwrap().clone()));
                    effects.push(IF::Pop(value.clone()));
                }
                let value = TraceValue::Value {
                    value: MoveValue::Variant(MoveVariant {
                        type_: vlayouts.type_.clone(),
                        fields: variant_fields,
                        variant_name: name_tag.0.clone(),
                        tag: name_tag.1,
                    }),
                };
                effects.push(IF::Push(value.clone()));
                self.trace.instruction(vec![], effects, remaining_gas, pc);
                self.current_stack.push(value);
            }
            B::PackVariantGeneric(vidx) => {
                let resolver = frame
                    .function
                    .get_resolver(self.link_context.unwrap(), loader);
                let (field_count, variant_tag) =
                    resolver.variant_instantiantiation_field_count_and_tag(*vidx);
                let stack_len = self.current_stack.len();
                let ty = loader
                    .type_to_fully_annotated_layout(
                        &resolver
                            .instantiate_enum_type(*vidx, &frame.ty_args)
                            .unwrap(),
                    )
                    .unwrap();
                let MoveTypeLayout::Enum(vlayouts) = &ty else {
                    panic!("Expected variant, got {:?}", ty);
                };
                let (name_tag, variant_layout) = vlayouts
                    .variants
                    .iter()
                    .find(|((_, tag), _)| *tag == variant_tag)
                    .unwrap()
                    .clone();
                let fields = self
                    .current_stack
                    .split_off(stack_len - field_count as usize);
                let mut effects = vec![];
                let mut variant_fields = vec![];
                assert!(
                    fields.len() == variant_layout.len(),
                    "Popped fields and variant fields mismatch"
                );
                for (value, field_layout) in fields.iter().zip(variant_layout.iter()) {
                    variant_fields
                        .push((field_layout.name.clone(), value.value().unwrap().clone()));
                    effects.push(IF::Pop(value.clone()));
                }
                let value = TraceValue::Value {
                    value: MoveValue::Variant(MoveVariant {
                        type_: vlayouts.type_.clone(),
                        fields: variant_fields,
                        variant_name: name_tag.0.clone(),
                        tag: name_tag.1,
                    }),
                };
                effects.push(IF::Push(value.clone()));
                // TODO: type args
                self.trace.instruction(vec![], effects, remaining_gas, pc);
                self.current_stack.push(value);
            }
            B::UnpackVariant(_) | B::UnpackVariantGeneric(_) => {
                let value = self.current_stack.pop().unwrap();
                let MoveValue::Variant(v) = value.value().unwrap().clone() else {
                    panic!("Expected variant, got {:#}", value.value().unwrap());
                };

                let mut effects = vec![IF::Pop(value.clone())];
                for (_, field_value) in v.fields.iter() {
                    let value = TraceValue::Value {
                        value: field_value.clone(),
                    };
                    effects.push(IF::Push(value.clone()));
                    self.current_stack.push(value);
                }
                self.trace.instruction(vec![], effects, remaining_gas, pc);
            }
            i @ (B::UnpackVariantImmRef(_)
            | B::UnpackVariantMutRef(_)
            | B::UnpackVariantGenericImmRef(_)
            | B::UnpackVariantGenericMutRef(_)) => {
                let value = self.current_stack.pop().unwrap();
                let location = value.location().unwrap().clone();
                let curr_value = self.copy_root_value(&location);
                let MoveValue::Variant(v) = curr_value.value().unwrap().clone() else {
                    panic!("Expected variant, got {:#}", value.value().unwrap());
                };

                let mut effects = vec![IF::Pop(value.clone())];
                for (field_idx, (_, field_value)) in v.fields.iter().enumerate() {
                    let value = match i {
                        B::UnpackVariantImmRef(_) | B::UnpackVariantGenericImmRef(_) => {
                            TraceValue::ImmRef {
                                location: Location::Indexed(Box::new(location.clone()), field_idx),
                                snapshot: Box::new(TraceValue::Value {
                                    value: field_value.clone(),
                                }),
                            }
                        }
                        B::UnpackVariantMutRef(_) | B::UnpackVariantGenericMutRef(_) => {
                            TraceValue::MutRef {
                                location: Location::Indexed(Box::new(location.clone()), field_idx),
                                snapshot: Box::new(TraceValue::Value {
                                    value: field_value.clone(),
                                }),
                            }
                        }
                        _ => unreachable!(),
                    };
                    effects.push(IF::Push(value.clone()));
                    self.current_stack.push(value);
                }
            }
            B::VariantSwitch(_) => {
                let v = self.current_stack.pop().unwrap();
                let effects = vec![IF::Pop(v.clone())];
                self.trace.instruction(vec![], effects, remaining_gas, pc);
            }
            B::ExistsDeprecated(_)
            | B::ExistsGenericDeprecated(_)
            | B::MoveFromDeprecated(_)
            | B::MoveFromGenericDeprecated(_)
            | B::MoveToDeprecated(_)
            | B::MoveToGenericDeprecated(_)
            | B::MutBorrowGlobalDeprecated(_)
            | B::MutBorrowGlobalGenericDeprecated(_)
            | B::ImmBorrowGlobalDeprecated(_)
            | B::ImmBorrowGlobalGenericDeprecated(_) => unreachable!(),
        }
    }

    fn close_instruction(
        &mut self,
        _pc: u16,
        _function: &Function,
        _loader: &Loader,
        _remaining_gas: u64,
    ) {
        ()
    }
}

impl Drop for InnerTypeTracer {
    fn drop(&mut self) {
        // println!("{:#?}", self.trace.borrow());
        println!("{}", serde_json::to_string_pretty(&self.trace).unwrap());
        self.trace.reconstruct();
    }
}

// all types fully substituted
#[derive(Debug, Clone)]
struct FunctionTypeInfo {
    ty_args: Vec<TypeTag>,
    local_types: Vec<(MoveTypeLayout, Option<RefType>)>,
    return_types: Vec<TypeTagWithRefs>,
}

fn deref_ty(ty: Type) -> (Type, Option<RefType>) {
    match ty {
        Type::Reference(r) => (*r, Some(RefType::Imm)),
        Type::MutableReference(t) => (*t, Some(RefType::Mut)),
        Type::TyParam(_) => unreachable!("Type parameters should be fully substituted"),
        _ => (ty, None),
    }
}

fn get_function_types(
    function: &Function,
    loader: &Loader,
    ty_args: &[Type],
    link_context: AccountAddress,
) -> FunctionTypeInfo {
    let (module, _) = loader.get_module(link_context, function.module_id());
    let fdef = module.function_def_at(function.index());
    let f_handle = module.function_handle_at(fdef.function);
    // let mut local_types = vec![];
    // let locals = module.signature_at(fdef.code.unwrap().locals);
    let local_types = {
        let signatures = &module.signature_at(fdef.code.as_ref().unwrap().locals).0;
        signatures
            .iter()
            .map(|tok| loader.make_type(&module, tok).unwrap())
            .map(|ty| loader.subst(&ty, ty_args).unwrap())
            .map(|ty| {
                let (ty, ref_type) = deref_ty(ty);
                (
                    loader.type_to_fully_annotated_layout(&ty).unwrap(),
                    ref_type,
                )
            })
            .collect::<Vec<_>>()
    };

    let return_types = {
        let signatures = &module.signature_at(f_handle.return_).0;
        signatures
            .iter()
            .map(|tok| loader.make_type(&module, tok).unwrap())
            .map(|ty| loader.subst(&ty, ty_args).unwrap())
            .map(|ty| {
                let (ty, ref_type) = deref_ty(ty);
                TypeTagWithRefs {
                    layout: loader.type_to_type_tag(&ty).unwrap(),
                    ref_type,
                }
            })
            .collect::<Vec<_>>()
    };

    let ty_args = ty_args
        .iter()
        .cloned()
        .map(|ty| {
            let (ty, ref_type) = deref_ty(ty);
            assert!(ref_type.is_none());
            loader.type_to_type_tag(&ty).unwrap()
        })
        .collect();

    FunctionTypeInfo {
        ty_args,
        local_types,
        return_types,
    }
}

fn get_constant(
    function: &Function,
    loader: &Loader,
    link_context: AccountAddress,
    const_idx: ConstantPoolIndex,
) -> MoveValue {
    let (module, _loaded_module) = loader.get_module(link_context, function.module_id());
    let constant = module.constant_at(const_idx);
    let ty = loader.make_type(&module, &constant.type_).unwrap();
    let ty = loader.type_to_fully_annotated_layout(&ty).unwrap();
    let value = MoveValue::simple_deserialize(&constant.data, &ty).unwrap();
    value
}

fn to_integer_value(v: &MoveValue) -> Option<IntegerValue> {
    Some(match v {
        MoveValue::U8(u) => IntegerValue::U8(*u),
        MoveValue::U16(u) => IntegerValue::U16(*u),
        MoveValue::U32(u) => IntegerValue::U32(*u),
        MoveValue::U64(u) => IntegerValue::U64(*u),
        MoveValue::U128(u) => IntegerValue::U128(*u),
        MoveValue::U256(u) => IntegerValue::U256(*u),
        _ => return None,
    })
}

fn from_integer_value(v: IntegerValue) -> MoveValue {
    match v {
        IntegerValue::U8(u) => MoveValue::U8(u),
        IntegerValue::U16(u) => MoveValue::U16(u),
        IntegerValue::U32(u) => MoveValue::U32(u),
        IntegerValue::U64(u) => MoveValue::U64(u),
        IntegerValue::U128(u) => MoveValue::U128(u),
        IntegerValue::U256(u) => MoveValue::U256(u),
    }
}
