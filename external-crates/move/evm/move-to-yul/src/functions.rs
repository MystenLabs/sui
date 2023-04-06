// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{context::Context, yul_functions, yul_functions::YulFunction, Generator};
use itertools::Itertools;
use move_model::{
    ast::TempIndex,
    emit, emitln,
    model::{FunId, ModuleId, QualifiedInstId, StructId},
    ty::{PrimitiveType, Type},
};
use move_stackless_bytecode::{
    function_target::FunctionTarget,
    function_target_pipeline::FunctionVariant,
    stackless_bytecode::{Bytecode, Constant, Label, Operation},
    stackless_control_flow_graph::{BlockContent, BlockId, StacklessControlFlowGraph},
};
use std::collections::{btree_map::Entry, BTreeMap};

/// Mutable state of the function generator.
pub(crate) struct FunctionGenerator<'a> {
    /// The parent generator.
    pub(crate) parent: &'a mut Generator,
    /// A mapping from locals of the currently compiled functions from which a reference is
    /// borrowed, to the position in a memory region where these locals have evaded to.
    /// All borrowed_locals have a consecutive position in this mapping, starting at zero.
    borrowed_locals: BTreeMap<TempIndex, usize>,
}

impl<'a> FunctionGenerator<'a> {
    /// Run the function generator for the given function
    pub fn run(parent: &'a mut Generator, ctx: &Context, fun_id: &QualifiedInstId<FunId>) {
        let mut fun_gen = Self {
            parent,
            borrowed_locals: Default::default(),
        };
        fun_gen.function(ctx, fun_id);
    }

    /// Generate Yul function for Move function.
    fn function(&mut self, ctx: &Context, fun_id: &QualifiedInstId<FunId>) {
        let fun = &ctx.env.get_function(fun_id.to_qualified_id());
        // TODO: change back to is_native_or_intrinsic if we decide to implement
        // intrinsic functions (such as reverse, contains, is_empty) in the Vector module as well
        // TODO: create a compilation flag to gate "send_"
        if fun.is_native() || fun.get_full_name_str().contains("send_") {
            // Special treatment for native functions, which have custom generators.
            ctx.native_funs.gen_native_function(self, ctx, fun_id);
            return;
        }

        let target = &ctx.targets.get_target(fun, &FunctionVariant::Baseline);

        // Emit function header
        let params = (0..target.get_parameter_count())
            .map(|idx| ctx.make_local_name(target, idx))
            .join(", ");
        let results = if target.get_return_count() == 0 {
            "".to_string()
        } else {
            format!(
                " -> {}",
                (0..target.get_return_count())
                    .map(|i| ctx.make_result_name(target, i))
                    .join(", ")
            )
        };
        emit!(
            ctx.writer,
            "function {}({}){} ",
            ctx.make_function_name(fun_id),
            params,
            results,
        );
        ctx.emit_block(|| {
            // Emit function locals
            self.collect_borrowed_locals(ctx, target);
            let locals = (target.get_parameter_count()..target.get_local_count())
                // filter locals which are not evaded to memory
                .filter(|idx| !self.borrowed_locals.contains_key(idx))
                .map(|idx| ctx.make_local_name(target, idx))
                .join(", ");
            if !locals.is_empty() {
                emitln!(ctx.writer, "let {}", locals);
            }
            if !self.borrowed_locals.is_empty() {
                // These locals are evaded to memory, as references to them are borrowed.
                // Allocate a chunk of memory for them.
                self.parent.call_builtin_with_result(
                    ctx,
                    "let ",
                    std::iter::once("$locals".to_string()),
                    YulFunction::Malloc,
                    std::iter::once(
                        (self.borrowed_locals.len() * yul_functions::WORD_SIZE).to_string(),
                    ),
                );
                // For all evaded locals which are parameters, we need to initialize them from
                // the Yul parameter.
                for idx in self.borrowed_locals.keys() {
                    if *idx < target.get_parameter_count() {
                        self.assign(ctx, target, *idx, ctx.make_local_name(target, *idx))
                    }
                }
            }

            // Compute control flow graph, entry block, and label map
            let code = target.data.code.as_slice();
            let cfg = StacklessControlFlowGraph::new_forward(code);
            let entry_bb = Self::get_actual_entry_block(&cfg);
            let label_map = Self::compute_label_map(&cfg, code);

            // Emit state machine to represent control flow.
            // TODO: Eliminate the need for this, see also
            //    https://medium.com/leaningtech/solving-the-structured-control-flow-problem-once-and-for-all-5123117b1ee2
            if cfg.successors(entry_bb).iter().all(|b| cfg.is_dummmy(*b)) {
                // In this trivial case, we have only one block and can omit the state machine
                if let BlockContent::Basic { lower, upper } = cfg.content(entry_bb) {
                    for offs in *lower..*upper + 1 {
                        self.bytecode(ctx, fun_id, target, &label_map, &code[offs as usize], false);
                    }
                } else {
                    panic!("effective entry block is not basic")
                }
            } else {
                emitln!(ctx.writer, "let $block := {}", entry_bb);
                emit!(ctx.writer, "for {} true {} ");
                ctx.emit_block(|| {
                    emitln!(ctx.writer, "switch $block");
                    for blk_id in &cfg.blocks() {
                        if let BlockContent::Basic { lower, upper } = cfg.content(*blk_id) {
                            // Emit code for this basic block.
                            emit!(ctx.writer, "case {} ", blk_id);
                            ctx.emit_block(|| {
                                for offs in *lower..*upper + 1 {
                                    self.bytecode(
                                        ctx,
                                        fun_id,
                                        target,
                                        &label_map,
                                        &code[offs as usize],
                                        true,
                                    );
                                }
                            })
                        }
                    }
                })
            }
        });
        emitln!(ctx.writer)
    }

    /// Compute the locals in the given function which are borrowed from and which are not
    /// already indirections to memory (like structs or vectors) Such locals need
    /// to be evaded to memory and cannot be kept on the stack, so we can create references
    /// to them.
    fn collect_borrowed_locals(&mut self, ctx: &Context, target: &FunctionTarget) {
        let mut mem_pos = 0;
        for bc in &target.data.code {
            if let Bytecode::Call(_, _, Operation::BorrowLoc, srcs, _) = bc {
                let ty = target.get_local_type(srcs[0]);
                if !ctx.type_is_struct(ty) {
                    if let Entry::Vacant(e) = self.borrowed_locals.entry(srcs[0]) {
                        e.insert(mem_pos);
                        mem_pos += 1
                    }
                }
            }
        }
    }

    /// Get the actual entry block, skipping trailing dummy blocks.
    fn get_actual_entry_block(cfg: &StacklessControlFlowGraph) -> BlockId {
        let mut entry_bb = cfg.entry_block();
        while cfg.is_dummmy(entry_bb) {
            assert_eq!(cfg.successors(entry_bb).len(), 1);
            entry_bb = *cfg.successors(entry_bb).iter().last().unwrap();
        }
        entry_bb
    }

    /// Compute a map from labels to block ids which those labels start.
    fn compute_label_map(
        cfg: &StacklessControlFlowGraph,
        code: &[Bytecode],
    ) -> BTreeMap<Label, BlockId> {
        let mut map = BTreeMap::new();
        for id in cfg.blocks() {
            if let BlockContent::Basic { lower, .. } = cfg.content(id) {
                if let Bytecode::Label(_, l) = &code[*lower as usize] {
                    map.insert(*l, id);
                }
            }
        }
        map
    }
}

// ================================================================================================
// Bytecode

impl<'a> FunctionGenerator<'a> {
    /// Generate Yul statement for a bytecode.
    fn bytecode(
        &mut self,
        ctx: &Context,
        fun_id: &QualifiedInstId<FunId>,
        target: &FunctionTarget,
        label_map: &BTreeMap<Label, BlockId>,
        bc: &Bytecode,
        has_flow: bool,
    ) {
        use Bytecode::*;
        emitln!(
            ctx.writer,
            "// {}",
            bc.display(target, &BTreeMap::default())
        );
        let print_loc = || {
            if ctx.options.generate_source_info() {
                let loc = target.get_bytecode_loc(bc.get_attr_id());
                emitln!(
                    ctx.writer,
                    "/// @src {}:{}:{}",
                    ctx.file_id_map
                        .get(&loc.file_id())
                        .expect("file id defined")
                        .0,
                    loc.span().start(),
                    loc.span().end()
                );
            }
        };
        let get_block = |l| label_map.get(l).expect("label has corresponding block");
        // Need to make a clone below to avoid cascading borrow problems. We don't want the
        // subsequent lambdas to access self.
        let borrowed_locals = self.borrowed_locals.clone();
        let local = |l: &TempIndex| {
            if let Some(ptr) = Self::local_ptr(&borrowed_locals, *l) {
                format!("mload({})", ptr)
            } else {
                ctx.make_local_name(target, *l)
            }
        };
        let make_struct_id = |m: &ModuleId, s: &StructId, inst: &[Type]| {
            m.qualified(*s)
                .instantiate(Type::instantiate_slice(inst, &fun_id.inst))
        };
        let get_local_type = |idx: TempIndex| target.get_local_type(idx).instantiate(&fun_id.inst);
        let mut builtin = |yul_fun: YulFunction, dest: &[TempIndex], srcs: &[TempIndex]| {
            print_loc();
            emitln!(
                ctx.writer,
                "{} := {}",
                local(&dest[0]),
                self.parent
                    .call_builtin_str(ctx, yul_fun, srcs.iter().map(local))
            )
        };
        let mut builtin_typed = |yul_fun_u8: YulFunction,
                                 yul_fun_u64: YulFunction,
                                 yul_fun_u128: YulFunction,
                                 yul_fun_u256: YulFunction,
                                 dest: &[TempIndex],
                                 srcs: &[TempIndex]| {
            use PrimitiveType::*;
            use Type::*;
            match get_local_type(srcs[0]) {
                Primitive(U8) => builtin(yul_fun_u8, dest, srcs),
                Primitive(U64) => builtin(yul_fun_u64, dest, srcs),
                Primitive(U128) => builtin(yul_fun_u128, dest, srcs),
                Struct(mid, sid, _) => {
                    if ctx.is_u256(mid.qualified(sid)) {
                        builtin(yul_fun_u256, dest, srcs)
                    } else {
                        panic!("unexpected operand type")
                    }
                }
                _ => panic!("unexpected operand type"),
            }
        };
        match bc {
            Jump(_, l) => {
                print_loc();
                emitln!(ctx.writer, "$block := {}", get_block(l))
            }
            Branch(_, if_t, if_f, cond) => {
                print_loc();
                emitln!(
                    ctx.writer,
                    "switch {}\n\
                     case 0  {{ $block := {} }}\n\
                     default {{ $block := {} }}",
                    local(cond),
                    get_block(if_f),
                    get_block(if_t),
                )
            }
            Assign(_, dest, src, _) => {
                print_loc();
                self.assign(ctx, target, *dest, local(src))
            }
            Load(_, dest, cons) => {
                print_loc();
                self.constant(ctx, local(dest), cons)
            }
            Ret(_, results) => {
                print_loc();
                for (idx, result) in results.iter().enumerate() {
                    emitln!(
                        ctx.writer,
                        "{} := {}",
                        ctx.make_result_name(target, idx),
                        local(result)
                    );
                }
                if !self.borrowed_locals.is_empty() {
                    // Free memory allocated for evaded locals
                    self.parent.call_builtin(
                        ctx,
                        YulFunction::Free,
                        vec![
                            "$locals".to_string(),
                            (self.borrowed_locals.len() * yul_functions::WORD_SIZE).to_string(),
                        ]
                        .into_iter(),
                    );
                }
                if has_flow {
                    emitln!(ctx.writer, "leave")
                }
            }
            Abort(_, code) => {
                print_loc();
                self.parent
                    .call_builtin(ctx, YulFunction::Abort, std::iter::once(local(code)))
            }
            Call(_, dest, op, srcs, _) => {
                use Operation::*;
                match op {
                    // Move function call
                    Function(m, f, inst) => {
                        print_loc();
                        let fun_id = m
                            .qualified(*f)
                            .instantiate(Type::instantiate_slice(inst, &fun_id.inst));
                        let fun = &ctx.env.get_function(fun_id.to_qualified_id());
                        if fun.get_name_string().contains("send__")
                            || fun.get_name_string().contains("bcs::to_bytes")
                        {
                            return;
                        }
                        self.move_call(ctx, target, dest, fun_id, srcs.iter().map(local))
                    }

                    // Packing and unpacking of structs
                    Pack(m, s, inst) => {
                        print_loc();
                        self.pack(
                            ctx,
                            target,
                            make_struct_id(m, s, inst),
                            dest[0],
                            srcs.iter().map(local),
                        )
                    }
                    Unpack(m, s, inst) => {
                        print_loc();
                        self.unpack(
                            ctx,
                            target,
                            make_struct_id(m, s, inst),
                            dest,
                            local(&srcs[0]),
                        )
                    }
                    Destroy => {
                        print_loc();
                        self.destroy(ctx, &get_local_type(srcs[0]), local(&srcs[0]))
                    }

                    // Resource management
                    MoveTo(m, s, inst) => {
                        print_loc();
                        self.parent.move_to(
                            ctx,
                            make_struct_id(m, s, inst),
                            local(&srcs[1]),
                            local(&srcs[0]),
                        )
                    }
                    MoveFrom(m, s, inst) => {
                        print_loc();
                        self.move_from(
                            ctx,
                            target,
                            make_struct_id(m, s, inst),
                            dest[0],
                            local(&srcs[0]),
                        )
                    }
                    Exists(m, s, inst) => {
                        print_loc();
                        self.exists(
                            ctx,
                            target,
                            make_struct_id(m, s, inst),
                            dest[0],
                            local(&srcs[0]),
                        )
                    }
                    BorrowGlobal(m, s, inst) => {
                        print_loc();
                        self.borrow_global(
                            ctx,
                            target,
                            make_struct_id(m, s, inst),
                            dest[0],
                            local(&srcs[0]),
                        )
                    }

                    // References
                    BorrowLoc => {
                        print_loc();
                        self.borrow_loc(ctx, target, dest, srcs)
                    }
                    BorrowField(m, s, inst, f) => {
                        print_loc();
                        self.borrow_field(
                            ctx,
                            get_local_type(dest[0]).skip_reference(),
                            make_struct_id(m, s, inst),
                            *f,
                            local(&dest[0]),
                            local(&srcs[0]),
                        )
                    }
                    ReadRef => {
                        print_loc();
                        self.read_ref(
                            ctx,
                            target,
                            &get_local_type(dest[0]),
                            dest[0],
                            local(&srcs[0]),
                        )
                    }
                    WriteRef => {
                        print_loc();
                        self.write_ref(
                            ctx,
                            &get_local_type(srcs[1]),
                            local(&srcs[0]),
                            local(&srcs[1]),
                        )
                    }
                    // FreezeRef transforms a mutable reference to an immutable one so just
                    // treat it as an assignment.
                    FreezeRef => {
                        print_loc();
                        self.assign(ctx, target, dest[0], local(&srcs[0]))
                    }

                    // Arithmetics
                    CastU8 => builtin(YulFunction::CastU8, dest, srcs),
                    CastU64 => builtin(YulFunction::CastU64, dest, srcs),
                    CastU128 => builtin(YulFunction::CastU128, dest, srcs),
                    CastU256 => builtin(YulFunction::CastU256, dest, srcs),
                    Not => builtin(YulFunction::LogicalNot, dest, srcs),
                    Add => builtin_typed(
                        YulFunction::AddU8,
                        YulFunction::AddU64,
                        YulFunction::AddU128,
                        YulFunction::AddU256,
                        dest,
                        srcs,
                    ),
                    Sub => builtin(YulFunction::Sub, dest, srcs),
                    Mul => builtin_typed(
                        YulFunction::MulU8,
                        YulFunction::MulU64,
                        YulFunction::MulU128,
                        YulFunction::MulU256,
                        dest,
                        srcs,
                    ),
                    Div => builtin(YulFunction::Div, dest, srcs),
                    Mod => builtin(YulFunction::Mod, dest, srcs),
                    BitOr => builtin(YulFunction::BitOr, dest, srcs),
                    BitAnd => builtin(YulFunction::BitAnd, dest, srcs),
                    Xor => builtin(YulFunction::BitXor, dest, srcs),
                    Shl => builtin_typed(
                        YulFunction::ShlU8,
                        YulFunction::ShlU64,
                        YulFunction::ShlU128,
                        YulFunction::ShlU256,
                        dest,
                        srcs,
                    ),
                    Shr => builtin(YulFunction::Shr, dest, srcs),
                    Lt => builtin(YulFunction::Lt, dest, srcs),
                    Gt => builtin(YulFunction::Gt, dest, srcs),
                    Le => builtin(YulFunction::LtEq, dest, srcs),
                    Ge => builtin(YulFunction::GtEq, dest, srcs),
                    Or => builtin(YulFunction::LogicalOr, dest, srcs),
                    And => builtin(YulFunction::LogicalAnd, dest, srcs),
                    // TODO: implement equality comparison for structs, vectors of structs and
                    // vectors of vectors
                    Eq => {
                        let src_type = get_local_type(srcs[0]);
                        print_loc();
                        emitln!(
                            ctx.writer,
                            "{} := {}({}, {})",
                            local(&dest[0]),
                            self.parent.equality_function(ctx, src_type),
                            local(&srcs[0]),
                            local(&srcs[1]),
                        );
                    }
                    Neq => {
                        print_loc();
                        let src_type = get_local_type(srcs[0]);
                        if ctx.type_allocates_memory(&src_type) {
                            let equality_call = format!(
                                "{}({}, {})",
                                self.parent.equality_function(ctx, src_type),
                                local(&srcs[0]),
                                local(&srcs[1])
                            );
                            emitln!(
                                ctx.writer,
                                "{} := {}",
                                local(&dest[0]),
                                self.parent.call_builtin_str(
                                    ctx,
                                    YulFunction::LogicalNot,
                                    std::iter::once(equality_call)
                                )
                            );
                        } else {
                            builtin(YulFunction::Neq, dest, srcs)
                        }
                    }
                    // Specification or other operations which can be ignored here
                    GetField(_, _, _, _)
                    | GetGlobal(_, _, _)
                    | IsParent(_, _)
                    | WriteBack(_, _)
                    | UnpackRef
                    | PackRef
                    | UnpackRefDeep
                    | PackRefDeep
                    | TraceLocal(_)
                    | TraceReturn(_)
                    | TraceAbort
                    | TraceExp(_, _)
                    | EmitEvent
                    | EventStoreDiverge
                    | OpaqueCallBegin(_, _, _)
                    | OpaqueCallEnd(_, _, _)
                    | Uninit
                    | Havoc(_)
                    | Stop
                    | TraceGlobalMem(_)
                    | CastU16
                    | CastU32 => {}
                }
            }

            Label(_, _) | Nop(_) | SaveMem(_, _, _) | SaveSpecVar(_, _, _) | Prop(_, _, _) => {
                // These opcodes are not needed, ignore them
            }
        }
    }

    fn assign(&self, ctx: &Context, target: &FunctionTarget, dest: TempIndex, src: String) {
        if let Some(ptr) = Self::local_ptr(&self.borrowed_locals, dest) {
            emitln!(ctx.writer, "mstore({}, {})", ptr, src)
        } else {
            emitln!(
                ctx.writer,
                "{} := {}",
                ctx.make_local_name(target, dest),
                src
            );
        }
    }

    /// Generate a string representing a constant.
    fn constant(&mut self, ctx: &Context, dest: String, cons: &Constant) {
        let val_str = match cons {
            Constant::Bool(v) => {
                if *v {
                    "true".to_string()
                } else {
                    "false".to_string()
                }
            }
            Constant::U8(v) => {
                format!("{}", v)
            }
            Constant::U64(v) => {
                format!("{}", v)
            }
            Constant::U128(v) => {
                format!("{}", v)
            }
            Constant::U256(v) => {
                format!("{}", v)
            }
            Constant::Address(a) => {
                format!("0x{}", a.to_str_radix(16))
            }
            Constant::ByteArray(_) => "".to_string(),
            Constant::AddressArray(_) => "".to_string(),
            Constant::Vector(_) => "".to_string(),
            Constant::U16(_) | Constant::U32(_) => panic!("unexpected field type"),
        };
        if !val_str.is_empty() {
            emitln!(ctx.writer, "{} := {}", dest, val_str);
        } else if let Constant::ByteArray(value) = cons {
            let compute_capacity_str = self.parent.call_builtin_str(
                ctx,
                YulFunction::ClosestGreaterPowerOfTwo,
                std::iter::once(value.len().to_string()),
            );
            let malloc_str = self.parent.call_builtin_str(
                ctx,
                YulFunction::Malloc,
                std::iter::once(format!("add(32, {})", compute_capacity_str)),
            );
            let store_length_str = self.parent.call_builtin_str(
                ctx,
                YulFunction::MemoryStoreU64,
                vec![dest.clone(), value.len().clone().to_string()].into_iter(),
            );
            let store_capacity_str = self.parent.call_builtin_str(
                ctx,
                YulFunction::MemoryStoreU64,
                vec![format!("add({}, 8)", dest), compute_capacity_str].into_iter(),
            );
            emitln!(ctx.writer, "{} := {}", dest, malloc_str);
            emitln!(ctx.writer, &store_length_str);
            emitln!(ctx.writer, &store_capacity_str);
            let fun_name = self.parent.copy_literal_to_memory(value.clone());
            emitln!(ctx.writer, "{}(add({}, 32))", fun_name, dest);
        }
    }

    /// Generate call to a Move function.
    fn move_call(
        &mut self,
        ctx: &Context,
        target: &FunctionTarget,
        dest: &[TempIndex],
        fun: QualifiedInstId<FunId>,
        mut args: impl Iterator<Item = String>,
    ) {
        let fun_name = ctx.make_function_name(&fun);
        self.parent.need_move_function(&fun);
        let call_str = format!("{}({})", fun_name, args.join(", "));
        match dest.len() {
            0 => emitln!(ctx.writer, &call_str),
            1 => self.assign(ctx, target, dest[0], call_str),
            _ => {
                if dest
                    .iter()
                    .any(|idx| self.borrowed_locals.contains_key(idx))
                {
                    // Multiple outputs, some of them evaded into memory. Need to introduce
                    // temporaries to store the result
                    ctx.emit_block(|| {
                        let temp_names = (0..dest.len())
                            .map(|i| {
                                if self.borrowed_locals.contains_key(&dest[i]) {
                                    format!("$d{}", i)
                                } else {
                                    ctx.make_local_name(target, dest[i])
                                }
                            })
                            .collect_vec();
                        emitln!(
                            ctx.writer,
                            "let {} := {}",
                            temp_names.iter().join(", "),
                            call_str
                        );
                        for (i, temp_name) in temp_names.into_iter().enumerate() {
                            if self.borrowed_locals.contains_key(&dest[i]) {
                                self.assign(ctx, target, dest[i], temp_name)
                            }
                        }
                    })
                } else {
                    let dest_str = dest
                        .iter()
                        .map(|idx| ctx.make_local_name(target, *idx))
                        .join(", ");
                    emitln!(ctx.writer, "{} := {}", dest_str, call_str)
                }
            }
        }
    }
}

// ================================================================================================
// Memory

impl<'a> FunctionGenerator<'a> {
    /// If this is a local which is borrowed and evaded to memory, return pointer to its memory.
    fn local_ptr(borrowed_locals: &BTreeMap<TempIndex, usize>, idx: TempIndex) -> Option<String> {
        borrowed_locals.get(&idx).map(|pos| {
            if *pos == 0 {
                "$locals".to_string()
            } else {
                format!("add($locals, {})", pos * yul_functions::WORD_SIZE)
            }
        })
    }

    /// Borrow a local.
    fn borrow_loc(
        &mut self,
        ctx: &Context,
        target: &FunctionTarget,
        dest: &[TempIndex],
        srcs: &[TempIndex],
    ) {
        let ty = target.get_local_type(srcs[0]);
        let mem_offs = if ctx.type_is_struct(ty) {
            // The values lives in memory and srcs[0] is an offset into this memory.
            ctx.make_local_name(target, srcs[0])
        } else {
            // A primitive which has been evaded to memory,
            // Need to adjust the offset for the local by (32 - size) to account for big endian.
            // We need to point to the actual highest byte of the value.
            let offs = (*self
                .borrowed_locals
                .get(&srcs[0])
                .expect("local evaded to memory")
                * yul_functions::WORD_SIZE)
                + 32
                - ctx.type_size(ty);
            if offs == 0 {
                "$locals".to_string()
            } else {
                format!("add($locals, {})", offs)
            }
        };
        let ref_value = self.parent.call_builtin_str(
            ctx,
            YulFunction::MakePtr,
            vec!["false".to_string(), mem_offs].into_iter(),
        );
        emitln!(
            ctx.writer,
            "{} := {}",
            ctx.make_local_name(target, dest[0]),
            ref_value
        );
    }

    /// Read the value of reference.
    fn read_ref(
        &mut self,
        ctx: &Context,
        target: &FunctionTarget,
        ty: &Type,
        dest_idx: TempIndex,
        src: String,
    ) {
        let load_str = self.parent.call_builtin_str(
            ctx,
            ctx.load_builtin_fun(ty.skip_reference()),
            std::iter::once(src.clone()),
        );
        self.assign(ctx, target, dest_idx, load_str);
        let is_storage_call =
            self.parent
                .call_builtin_str(ctx, YulFunction::IsStoragePtr, std::iter::once(src));
        let hash = self.parent.type_hash(ctx, ty);
        let stroage_ptr_name = format!("$storage_ptr_{}", hash);
        if ty.is_vector() || (ty.is_struct() && !ctx.is_u256_ty(ty)) {
            emit!(ctx.writer, "if {}", is_storage_call);
            ctx.emit_block(|| {
                emitln!(ctx.writer, "let {}", stroage_ptr_name);
                let dest = if let Some(ptr) = Self::local_ptr(&self.borrowed_locals, dest_idx) {
                    format!("mload({})", ptr)
                } else {
                    ctx.make_local_name(target, dest_idx)
                };
                self.parent.move_data_from_linked_storage(
                    ctx,
                    ty,
                    dest,
                    stroage_ptr_name.clone(),
                    false,
                );
                self.assign(ctx, target, dest_idx, stroage_ptr_name);
            })
        }
    }

    /// Write the value of reference.
    fn write_ref(&mut self, ctx: &Context, ty: &Type, dest: String, src: String) {
        let yul_fun = ctx.store_builtin_fun(ty.skip_reference());
        let is_storage_call = self.parent.call_builtin_str(
            ctx,
            YulFunction::IsStoragePtr,
            std::iter::once(dest.clone()),
        );
        let hash = self.parent.type_hash(ctx, ty);
        let stroage_ptr_name = format!("$storage_ptr_{}", hash);
        if ty.is_vector() || (ty.is_struct() && !ctx.is_u256_ty(ty)) {
            emit!(ctx.writer, "if {}", is_storage_call);
            ctx.emit_block(|| {
                self.parent.create_and_move_data_to_linked_storage(
                    ctx,
                    ty,
                    src.clone(),
                    stroage_ptr_name.clone(),
                    false,
                );
                emitln!(ctx.writer, "{} := {}", src, stroage_ptr_name);
            });
        }
        self.parent
            .call_builtin(ctx, yul_fun, vec![dest, src].into_iter())
    }

    /// Pack a structure.
    fn pack(
        &mut self,
        ctx: &Context,
        target: &FunctionTarget,
        struct_id: QualifiedInstId<StructId>,
        dest: TempIndex,
        srcs: impl Iterator<Item = String>,
    ) {
        let layout = ctx.get_struct_layout(&struct_id);

        ctx.emit_block(|| {
            // Allocate memory
            let mem = "$mem".to_string();
            emitln!(
                ctx.writer,
                "let $mem := {}",
                self.parent.call_builtin_str(
                    ctx,
                    YulFunction::Malloc,
                    std::iter::once(layout.size.to_string()),
                )
            );

            // Initialize fields
            let struct_env = &ctx.env.get_struct(struct_id.to_qualified_id());
            for (logical_offs, field_exp) in srcs.enumerate() {
                let yul_fun = ctx.memory_store_builtin_fun(
                    &struct_env
                        .get_field_by_offset(logical_offs)
                        .get_type()
                        .instantiate(&struct_id.inst),
                );
                let (byte_offset, _) = *layout.offsets.get(&logical_offs).unwrap();
                let field_ptr = format!("add({}, {})", mem, byte_offset);
                self.parent
                    .call_builtin(ctx, yul_fun, vec![field_ptr, field_exp].into_iter());
            }
            self.assign(ctx, target, dest, mem);
        })
    }

    /// Unpack a structure.
    fn unpack(
        &mut self,
        ctx: &Context,
        target: &FunctionTarget,
        struct_id: QualifiedInstId<StructId>,
        dest: &[TempIndex],
        src: String,
    ) {
        let layout = ctx.get_struct_layout(&struct_id);

        // Copy fields
        let struct_env = &ctx.env.get_struct(struct_id.to_qualified_id());
        for (logical_offs, dest_idx) in dest.iter().enumerate() {
            let yul_fun = ctx.memory_load_builtin_fun(
                &struct_env
                    .get_field_by_offset(logical_offs)
                    .get_type()
                    .instantiate(&struct_id.inst),
            );
            let (byte_offset, _) = *layout.offsets.get(&logical_offs).unwrap();
            let field_ptr = format!("add({}, {})", src, byte_offset);
            let call_str = self
                .parent
                .call_builtin_str(ctx, yul_fun, std::iter::once(field_ptr));
            self.assign(ctx, target, *dest_idx, call_str);
        }

        // Free memory
        self.parent.call_builtin(
            ctx,
            YulFunction::Free,
            vec![src, layout.size.to_string()].into_iter(),
        )
    }

    /// Destroy (free) a value of type.
    /// TODO: the Destroy instruction is currently not reflecting lifetime of values correctly,
    ///   but is only inserted for the original pop bytecode. We should run lifetime analysis
    ///   and insert Destroy where needed. However, this does not matter much yet, as
    ///   YulFunction::Free is a nop in the current runtime which uses arena style allocation.
    fn destroy(&mut self, ctx: &Context, ty: &Type, src: String) {
        use Type::*;
        match ty {
            Struct(m, s, inst) => {
                // Destroy all fields
                let struct_id = m.qualified(*s).instantiate(inst.clone());
                let layout = ctx.get_struct_layout(&struct_id);
                let struct_env = &ctx.env.get_struct(struct_id.to_qualified_id());
                for field in struct_env.get_fields() {
                    let field_type = field.get_type().instantiate(inst);
                    if !ctx.type_allocates_memory(&field_type) {
                        continue;
                    }
                    ctx.emit_block(|| {
                        let (byte_offset, _) = layout.offsets.get(&field.get_offset()).unwrap();
                        let field_ptr = self.parent.call_builtin_str(
                            ctx,
                            YulFunction::LoadU256,
                            std::iter::once(format!("add({}, {})", src, byte_offset)),
                        );
                        let field_ptr_name =
                            format!("$field_ptr_{}", self.parent.type_hash(ctx, &field_type));
                        emitln!(ctx.writer, "let {} := {}", field_ptr_name, field_ptr);
                        self.destroy(ctx, &field_type, field_ptr_name);
                    })
                }

                // Free the memory allocated by this struct.
                self.parent.call_builtin(
                    ctx,
                    YulFunction::Free,
                    vec![src, layout.size.to_string()].into_iter(),
                )
            }
            Vector(ty) => {
                if ctx.type_allocates_memory(ty.as_ref()) {
                    // TODO: implement vectors
                }
            }
            _ => {}
        }
    }

    /// Borrow a field, creating a reference to it.
    fn borrow_field(
        &mut self,
        ctx: &Context,
        _ty: &Type,
        struct_id: QualifiedInstId<StructId>,
        field_offs: usize,
        dest: String,
        src: String,
    ) {
        let layout = ctx.get_struct_layout(&struct_id);
        let (byte_offs, ty) = layout
            .offsets
            .get(&field_offs)
            .expect("field offset defined");
        let add_offset = if *byte_offs == 0 {
            src
        } else {
            self.parent.call_builtin_str(
                ctx,
                YulFunction::IndexPtr,
                vec![src, byte_offs.to_string()].into_iter(),
            )
        };
        if ctx.type_is_struct(ty) {
            // If this is an indirection to a struct or vector, load its value and make a ptr of it.
            ctx.emit_block(|| {
                let field_ptr = if *byte_offs == 0 {
                    add_offset
                } else {
                    emitln!(ctx.writer, "let $field_ptr := {}", add_offset);
                    "$field_ptr".to_string()
                };
                let is_storage_call = self.parent.call_builtin_str(
                    ctx,
                    YulFunction::IsStoragePtr,
                    std::iter::once(field_ptr.clone()),
                );
                let load_call = self.parent.call_builtin_str(
                    ctx,
                    YulFunction::LoadU256,
                    std::iter::once(field_ptr),
                );
                emitln!(
                    ctx.writer,
                    "{} := {}",
                    dest,
                    self.parent.call_builtin_str(
                        ctx,
                        YulFunction::MakePtr,
                        vec![is_storage_call, load_call].into_iter()
                    )
                )
            })
        } else {
            emitln!(ctx.writer, "{} := {}", dest, add_offset)
        }
    }

    /// Test whether resource exists.
    fn exists(
        &mut self,
        ctx: &Context,
        target: &FunctionTarget,
        struct_id: QualifiedInstId<StructId>,
        dst: TempIndex,
        addr: String,
    ) {
        let exists_check = self.parent.exists_check(ctx, struct_id, addr);
        self.assign(ctx, target, dst, exists_check);
    }

    /// Move resource from storage to local.
    fn move_from(
        &mut self,
        ctx: &Context,
        target: &FunctionTarget,
        struct_id: QualifiedInstId<StructId>,
        dst: TempIndex,
        addr: String,
    ) {
        ctx.emit_block(|| {
            // Obtain the storage base offset for this resource.
            emitln!(
                ctx.writer,
                "let $base_offset := {}",
                self.parent.type_storage_base(
                    ctx,
                    &struct_id.to_type(),
                    "${RESOURCE_STORAGE_CATEGORY}",
                    addr,
                )
            );
            let base_offset = "$base_offset";

            // At the base offset we store a boolean indicating whether the resource exists. Check this
            // and if it is not set, abort. Otherwise clear this bit.
            let exists_call = self.parent.call_builtin_str(
                ctx,
                YulFunction::AlignedStorageLoad,
                std::iter::once(base_offset.to_string()),
            );
            let abort_call =
                self.parent
                    .call_builtin_str(ctx, YulFunction::AbortBuiltin, std::iter::empty());
            emitln!(
                ctx.writer,
                "if iszero({}) {{\n  {}\n}}",
                exists_call,
                abort_call
            );
            self.parent.call_builtin(
                ctx,
                YulFunction::AlignedStorageStore,
                vec![base_offset.to_string(), "false".to_string()].into_iter(),
            );

            // Move the struct out of storage into memory
            ctx.emit_block(|| {
                // The actual resource data starts at base_offset + 32. Set the src address
                // to this.
                emitln!(
                    ctx.writer,
                    "let $src := add({}, ${{RESOURCE_EXISTS_FLAG_SIZE}})",
                    base_offset
                );

                // Perform the move and assign the result.
                emitln!(ctx.writer, "let $dst");
                self.parent.move_struct_to_memory(
                    ctx,
                    &struct_id,
                    "$src".to_string(),
                    "$dst".to_string(),
                    true,
                );
                self.assign(ctx, target, dst, "$dst".to_string())
            })
        })
    }

    /// Borrow a resource.
    fn borrow_global(
        &mut self,
        ctx: &Context,
        target: &FunctionTarget,
        struct_id: QualifiedInstId<StructId>,
        dst: TempIndex,
        addr: String,
    ) {
        ctx.emit_block(|| {
            let res = self.parent.borrow_global_instrs(ctx, &struct_id, addr);
            self.assign(ctx, target, dst, res);
        });
    }
}
