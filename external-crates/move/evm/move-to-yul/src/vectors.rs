// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// This file defines vector functionalities.

use crate::{
    context::Context, functions::FunctionGenerator, native_functions::NativeFunctions,
    yul_functions::YulFunction, Generator,
};
use move_model::{
    emitln,
    model::{FunId, QualifiedInstId},
    ty::Type,
};

/// The size (in bytes) of the vector metadata, which is stored in front of the actual vector data.
pub const VECTOR_METADATA_SIZE: usize = 32;
/// The number of slots allocated initially for an empty vector.
pub const VECTOR_INITIAL_CAPACITY: usize = 2;

impl Generator {
    pub(crate) fn move_vector_to_storage(
        &mut self,
        ctx: &Context,
        vector_type: &Type,
        src: String,
        dst: String,
        clean_flag: bool,
    ) {
        let elem_type = get_elem_type(vector_type).expect("not vector type");
        let elem_type_size = ctx.type_size(&elem_type);

        // Add hash to variable names to avoid naming collisions.
        let hash = self.type_hash(ctx, vector_type);
        let size_name = format!("$size_{}", hash);
        let offs_name = format!("$offs_{}", hash);
        let data_size_name = format!("$data_size_{}", hash);
        let data_src_name = format!("$data_src_{}", hash);
        let data_dst_name = format!("$data_dst_{}", hash);

        // Load size of the vectors
        emitln!(
            ctx.writer,
            "let {} := {}",
            size_name,
            self.call_builtin_str(
                ctx,
                YulFunction::MemoryLoadU64,
                std::iter::once(src.clone())
            )
        );

        // Calculate the size of all the data
        emitln!(
            ctx.writer,
            "let {} := mul({}, {})",
            data_size_name,
            size_name,
            elem_type_size
        );

        // Move vector metadata to the storage location
        self.call_builtin(
            ctx,
            YulFunction::AlignedStorageStore,
            vec![dst.clone(), format!("mload({})", src)].into_iter(),
        );

        // Calculate the start of actual vector data at memory and storage location
        emitln!(
            ctx.writer,
            "let {} := add({}, {})",
            data_src_name,
            src,
            VECTOR_METADATA_SIZE
        );
        emitln!(
            ctx.writer,
            "let {} := add({}, {})",
            data_dst_name,
            dst,
            VECTOR_METADATA_SIZE
        );

        // Loops through the vector and move elements
        emitln!(
            ctx.writer,
            "for {{ let {} := 0 }} lt({}, {}) {{ {} := add({}, 32)}} {{",
            offs_name,
            offs_name,
            data_size_name,
            offs_name,
            offs_name,
        );
        ctx.writer.indent();
        if ctx.type_allocates_memory(&elem_type) {
            ctx.emit_block(|| {
                let linked_src_name = format!("$linked_src_{}", self.type_hash(ctx, &elem_type));
                let linked_dst_name = format!("$linked_dst_{}", self.type_hash(ctx, &elem_type));

                // Load the pointer to the linked memory.
                emitln!(
                    ctx.writer,
                    "let {} := mload(add({}, {}))",
                    linked_src_name,
                    offs_name,
                    data_src_name.clone(),
                );
                self.create_and_move_data_to_linked_storage(
                    ctx,
                    &elem_type,
                    linked_src_name,
                    linked_dst_name.clone(),
                    clean_flag,
                );
                // Store the result at the destination
                self.call_builtin(
                    ctx,
                    YulFunction::AlignedStorageStore,
                    vec![
                        format!("add({}, {})", data_dst_name, offs_name),
                        linked_dst_name,
                    ]
                    .into_iter(),
                )
            });
        } else {
            self.call_builtin(
                ctx,
                YulFunction::AlignedStorageStore,
                vec![
                    format!("add({}, {})", data_dst_name, offs_name),
                    format!("mload(add({}, {}))", data_src_name, offs_name),
                ]
                .into_iter(),
            );
        }

        // Free ptr
        if clean_flag {
            self.call_builtin(
                ctx,
                YulFunction::Free,
                vec![
                    src,
                    format!("add({}, {})", data_size_name, VECTOR_METADATA_SIZE),
                ]
                .into_iter(),
            );
        }

        ctx.writer.unindent();
        emitln!(ctx.writer, "}");
    }

    pub(crate) fn move_vector_to_memory(
        &mut self,
        ctx: &Context,
        vector_type: &Type,
        src: String,
        dst: String,
        clean_flag: bool, // whether to clean the storage
    ) {
        let elem_type = get_elem_type(vector_type).expect("not vector type");
        let elem_type_size = ctx.type_size(&elem_type);

        // Add hash to variable names to avoid naming collisions.
        let hash = self.type_hash(ctx, vector_type);
        let size_name = format!("$size_{}", hash);
        let capacity_name = format!("$capacity_{}", hash);
        let data_size_name = format!("$data_size_{}", hash);
        let data_src_name = format!("$data_src_{}", hash);
        let data_dst_name = format!("$data_dst_{}", hash);
        let offs_name = format!("$offs_{}", hash);

        // Load size of the vector
        emitln!(
            ctx.writer,
            "let {} := {}",
            size_name,
            self.call_builtin_str(
                ctx,
                YulFunction::StorageLoadU64,
                std::iter::once(src.clone())
            )
        );

        // Calculate the closest power of two that's greater than the size we loaded on the
        // last line. We will allocate space for this number of elements in the memory.
        emitln!(
            ctx.writer,
            "let {} := {}",
            capacity_name,
            self.call_builtin_str(
                ctx,
                YulFunction::ClosestGreaterPowerOfTwo,
                std::iter::once(size_name.clone())
            )
        );

        emitln!(
            ctx.writer,
            "{} := {}",
            dst,
            self.call_builtin_str(
                ctx,
                YulFunction::Malloc,
                std::iter::once(format!(
                    "add({}, mul({}, {}))",
                    VECTOR_METADATA_SIZE, capacity_name, elem_type_size
                ))
            )
        );

        // Calculate size of the vector data
        emitln!(
            ctx.writer,
            "let {} := mul({}, {})",
            data_size_name,
            size_name,
            elem_type_size
        );

        // Move metadata to memory
        emitln!(
            ctx.writer,
            "mstore({}, {})",
            dst,
            self.call_builtin_str(
                ctx,
                YulFunction::AlignedStorageLoad,
                std::iter::once(src.clone()),
            )
        );

        // Store new capacity in memory
        self.call_builtin(
            ctx,
            YulFunction::MemoryStoreU64,
            vec![format!("add({}, 8)", dst), capacity_name].into_iter(),
        );

        // Calculate locations to load data from and move data to
        emitln!(
            ctx.writer,
            "let {} := add({}, {})",
            data_src_name,
            src,
            VECTOR_METADATA_SIZE
        );
        emitln!(
            ctx.writer,
            "let {} := add({}, {})",
            data_dst_name,
            dst,
            VECTOR_METADATA_SIZE
        );

        // Loop through the vector and move elements to memory
        emitln!(
            ctx.writer,
            "for {{ let {} := 0 }} lt({}, {}) {{ {} := add({}, 32)}} {{",
            offs_name,
            offs_name,
            data_size_name,
            offs_name,
            offs_name,
        );

        ctx.writer.indent();
        if ctx.type_allocates_memory(&elem_type) {
            ctx.emit_block(|| {
                let src_ptr = format!("add({}, {})", data_src_name, offs_name);
                let dst_ptr = format!("add({}, {})", data_dst_name, offs_name);
                let hash = self.type_hash(ctx, &elem_type);
                let linked_src_name = format!("$linked_src_{}", hash);
                let linked_dst_name = format!("$linked_dst_{}", hash);

                // Load the pointer to the linked storage.
                let load_call = self.call_builtin_str(
                    ctx,
                    YulFunction::AlignedStorageLoad,
                    std::iter::once(src_ptr.clone()),
                );

                emitln!(ctx.writer, "let {} := {}", linked_src_name, load_call);
                // Declare where to store the result and recursively move
                emitln!(ctx.writer, "let {}", linked_dst_name);
                self.move_data_from_linked_storage(
                    ctx,
                    &elem_type,
                    linked_src_name,
                    linked_dst_name.clone(),
                    clean_flag,
                );
                // Store the result at the destination.
                emitln!(ctx.writer, "mstore({}, {})", dst_ptr, linked_dst_name);
                // Clear the storage to get a refund
                if clean_flag {
                    self.call_builtin(
                        ctx,
                        YulFunction::AlignedStorageStore,
                        vec![src_ptr, 0.to_string()].into_iter(),
                    );
                }
            });
        } else {
            let load_call = self.call_builtin_str(
                ctx,
                YulFunction::AlignedStorageLoad,
                std::iter::once(format!("add({}, {})", data_src_name, offs_name)),
            );
            emitln!(
                ctx.writer,
                "mstore(add({}, {}), {})",
                data_dst_name,
                offs_name,
                load_call
            );
            // fill storage with 0s
            if clean_flag {
                self.call_builtin(
                    ctx,
                    YulFunction::AlignedStorageStore,
                    vec![
                        format!("add({}, {})", data_src_name, offs_name),
                        0.to_string(),
                    ]
                    .into_iter(),
                );
            }
        }
        ctx.writer.unindent();
        emitln!(ctx.writer, "}");
    }
}

impl NativeFunctions {
    /// Define vector functions for a specific instantiation.
    pub(crate) fn define_vector_functions(&mut self, ctx: &Context) {
        let vector = &self.find_module(ctx, "0x1", "vector");

        self.define(ctx, vector, "empty", crate::vectors::define_empty_fun);
        self.define(ctx, vector, "length", crate::vectors::define_length_fun);
        self.define(
            ctx,
            vector,
            "push_back",
            crate::vectors::define_push_back_fun,
        );
        self.define(ctx, vector, "pop_back", crate::vectors::define_pop_back_fun);
        self.define(ctx, vector, "borrow", crate::vectors::define_borrow_fun);
        self.define(ctx, vector, "borrow_mut", crate::vectors::define_borrow_fun);
        self.define(ctx, vector, "swap", crate::vectors::define_swap_fun);
        self.define(
            ctx,
            vector,
            "destroy_empty",
            crate::vectors::define_destroy_empty_fun,
        );
    }
}

fn define_empty_fun(gen: &mut FunctionGenerator, ctx: &Context, fun_id: &QualifiedInstId<FunId>) {
    assert_eq!(
        fun_id.inst.len(),
        1,
        "vector instantiated with non-one type parameter"
    );
    emitln!(ctx.writer, "() -> vector {");
    ctx.writer.indent();
    let type_size = ctx.type_size(fun_id.inst.get(0).unwrap());
    emitln!(
        ctx.writer,
        "vector := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::Malloc,
            std::iter::once(
                (VECTOR_METADATA_SIZE + type_size * VECTOR_INITIAL_CAPACITY).to_string()
            ),
        )
    );
    emitln!(
        ctx.writer,
        "{}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::MemoryStoreU64,
            vec![
                "add(vector, 8)".to_string(),
                VECTOR_INITIAL_CAPACITY.to_string()
            ]
            .into_iter()
        )
    );
    ctx.writer.unindent();
    emitln!(ctx.writer, "}");
}

fn define_length_fun(gen: &mut FunctionGenerator, ctx: &Context, _fun_id: &QualifiedInstId<FunId>) {
    emitln!(ctx.writer, "(v_ref) -> len {");
    ctx.writer.indent();
    emitln!(
        ctx.writer,
        "let v_offs := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::LoadU256,
            std::iter::once("v_ref".to_string())
        )
    );
    let is_storage_call = gen.parent.call_builtin_str(
        ctx,
        YulFunction::IsStoragePtr,
        std::iter::once("v_ref".to_string()),
    );
    emitln!(
        ctx.writer,
        "let v_ptr := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::MakePtr,
            vec![is_storage_call, "v_offs".to_string()].into_iter()
        )
    );
    emitln!(
        ctx.writer,
        "len := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::LoadU64,
            std::iter::once("v_ptr".to_string())
        )
    );
    ctx.writer.unindent();
    emitln!(ctx.writer, "}");
}

fn define_borrow_fun(gen: &mut FunctionGenerator, ctx: &Context, fun_id: &QualifiedInstId<FunId>) {
    assert_eq!(
        fun_id.inst.len(),
        1,
        "vector instantiated with non-one type parameter"
    );
    let elem_type = fun_id.inst.get(0).unwrap();
    let elem_type_size = ctx.type_size(elem_type);

    emitln!(ctx.writer, "(v_ref, i) -> e_ptr {");
    ctx.writer.indent();

    emitln!(
        ctx.writer,
        "let v_offs := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::LoadU256,
            std::iter::once("v_ref".to_string())
        )
    );
    let is_storage_call = gen.parent.call_builtin_str(
        ctx,
        YulFunction::IsStoragePtr,
        std::iter::once("v_ref".to_string()),
    );
    emitln!(
        ctx.writer,
        "let v_ptr := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::MakePtr,
            vec![is_storage_call.clone(), "v_offs".to_string()].into_iter()
        )
    );
    emitln!(
        ctx.writer,
        "let size := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::LoadU64,
            std::iter::once("v_ptr".to_string())
        )
    );

    emitln!(
        ctx.writer,
        "if {} {{ {} }}",
        &gen.parent.call_builtin_str(
            ctx,
            YulFunction::GtEq,
            vec!["i".to_string(), "size".to_string()].into_iter()
        ),
        &gen.parent
            .call_builtin_str(ctx, YulFunction::AbortBuiltin, std::iter::empty())
    );

    // calculate byte offset at which the new element should be stored
    emitln!(
        ctx.writer,
        "e_ptr := {}",
        &gen.parent.call_builtin_str(
            ctx,
            YulFunction::IndexPtr,
            vec![
                "v_ptr".to_string(),
                format!("add({}, mul(i, {}))", VECTOR_METADATA_SIZE, elem_type_size)
            ]
            .into_iter()
        )
    );
    if ctx.type_is_struct(elem_type) {
        emitln!(
            ctx.writer,
            "let e := {}",
            gen.parent.call_builtin_str(
                ctx,
                YulFunction::LoadU256,
                std::iter::once("e_ptr".to_string())
            )
        );
        emitln!(
            ctx.writer,
            "e_ptr := {}",
            gen.parent.call_builtin_str(
                ctx,
                YulFunction::MakePtr,
                vec![is_storage_call, "e".to_string()].into_iter()
            )
        );
    }
    ctx.writer.unindent();
    emitln!(ctx.writer, "}");
}

fn define_pop_back_fun(
    gen: &mut FunctionGenerator,
    ctx: &Context,
    fun_id: &QualifiedInstId<FunId>,
) {
    assert_eq!(
        fun_id.inst.len(),
        1,
        "vector instantiated with non-one type parameter"
    );
    let elem_type = fun_id.inst.get(0).unwrap();
    let elem_type_size = ctx.type_size(elem_type);

    emitln!(ctx.writer, "(v_ref) -> e {");
    ctx.writer.indent();

    emitln!(
        ctx.writer,
        "let v_offs := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::LoadU256,
            std::iter::once("v_ref".to_string())
        )
    );
    let is_storage_call = gen.parent.call_builtin_str(
        ctx,
        YulFunction::IsStoragePtr,
        std::iter::once("v_ref".to_string()),
    );
    emitln!(
        ctx.writer,
        "let v_ptr := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::MakePtr,
            vec![is_storage_call, "v_offs".to_string()].into_iter()
        )
    );

    emitln!(
        ctx.writer,
        "let size := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::LoadU64,
            std::iter::once("v_ptr".to_string())
        )
    );

    emitln!(
        ctx.writer,
        "if iszero(size) {{ {} }}",
        gen.parent
            .call_builtin_str(ctx, YulFunction::AbortBuiltin, std::iter::empty())
    );

    emitln!(
        ctx.writer,
        "let e_ptr := {}",
        &gen.parent.call_builtin_str(
            ctx,
            YulFunction::IndexPtr,
            vec![
                "v_ptr".to_string(),
                format!(
                    "add({}, mul(sub(size, 1), {}))",
                    VECTOR_METADATA_SIZE, elem_type_size
                )
            ]
            .into_iter()
        )
    );
    emitln!(
        ctx.writer,
        "e := {}",
        gen.parent.call_builtin_str(
            ctx,
            ctx.load_builtin_fun(elem_type),
            std::iter::once("e_ptr".to_string())
        )
    );
    // Move element from storage to memory if vector is in global storage and element is a struct or vector
    if ctx.type_allocates_memory(elem_type) {
        emitln!(
            ctx.writer,
            "if {} {{",
            gen.parent.call_builtin_str(
                ctx,
                YulFunction::IsStoragePtr,
                std::iter::once("e_ptr".to_string())
            ),
        );

        ctx.writer.indent();
        emitln!(
            ctx.writer,
            "let e_offs := {}",
            gen.parent.call_builtin_str(
                ctx,
                YulFunction::OffsetPtr,
                std::iter::once("e_ptr".to_string())
            ),
        );

        emitln!(
            ctx.writer,
            "let linked_src := {}",
            gen.parent.call_builtin_str(
                ctx,
                YulFunction::AlignedStorageLoad,
                std::iter::once("e_offs".to_string()),
            )
        );

        gen.parent.move_data_from_linked_storage(
            ctx,
            elem_type,
            "linked_src".to_string(),
            "e".to_string(),
            true,
        );

        gen.parent.call_builtin(
            ctx,
            YulFunction::AlignedStorageStore,
            vec!["e_offs".to_string(), 0.to_string()].into_iter(),
        );

        ctx.writer.unindent();
        emitln!(ctx.writer, "}");
    }

    emitln!(
        ctx.writer,
        &gen.parent.call_builtin_str(
            ctx,
            YulFunction::StoreU64,
            vec!["v_ptr".to_string(), "sub(size, 1)".to_string()].into_iter()
        )
    );

    ctx.writer.unindent();
    emitln!(ctx.writer, "}");
}

fn define_push_back_fun(
    gen: &mut FunctionGenerator,
    ctx: &Context,
    fun_id: &QualifiedInstId<FunId>,
) {
    assert_eq!(
        fun_id.inst.len(),
        1,
        "vector instantiated with non-one type parameter"
    );
    let elem_type = fun_id.inst.get(0).unwrap();
    let elem_type_size = ctx.type_size(elem_type);

    emitln!(ctx.writer, "(v_ref, e) {");
    ctx.writer.indent();
    emitln!(
        ctx.writer,
        "let v_offs := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::LoadU256,
            std::iter::once("v_ref".to_string())
        )
    );
    let is_storage_call = gen.parent.call_builtin_str(
        ctx,
        YulFunction::IsStoragePtr,
        std::iter::once("v_ref".to_string()),
    );
    emitln!(
        ctx.writer,
        "let v_ptr := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::MakePtr,
            vec![is_storage_call, "v_offs".to_string()].into_iter()
        )
    );

    emitln!(
        ctx.writer,
        "let size := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::LoadU64,
            std::iter::once("v_ptr".to_string())
        )
    );

    // calculate byte offset at which the new element should be stored
    emitln!(
        ctx.writer,
        "let e_ptr := {}",
        &gen.parent.call_builtin_str(
            ctx,
            YulFunction::IndexPtr,
            vec![
                "v_ptr".to_string(),
                format!(
                    "add({}, mul(size, {}))",
                    VECTOR_METADATA_SIZE, elem_type_size
                )
            ]
            .into_iter()
        )
    );

    // store the new element there
    emitln!(
        ctx.writer,
        &gen.parent.call_builtin_str(
            ctx,
            ctx.store_builtin_fun(elem_type),
            vec!["e_ptr".to_string(), "e".to_string()].into_iter()
        )
    );

    // Move element to storage if vector is in global storage and element is a struct or vector
    if ctx.type_allocates_memory(elem_type) {
        emitln!(
            ctx.writer,
            "if {} {{",
            gen.parent.call_builtin_str(
                ctx,
                YulFunction::IsStoragePtr,
                std::iter::once("e_ptr".to_string())
            ),
        );

        ctx.writer.indent();
        emitln!(
            ctx.writer,
            "let e_offs := {}",
            gen.parent.call_builtin_str(
                ctx,
                YulFunction::OffsetPtr,
                std::iter::once("e_ptr".to_string())
            ),
        );

        let linked_dst_name = format!("$linked_dst_{}", gen.parent.type_hash(ctx, elem_type));

        gen.parent.create_and_move_data_to_linked_storage(
            ctx,
            elem_type,
            "e".to_string(),
            linked_dst_name.clone(),
            true,
        );
        // Store the result at the destination
        gen.parent.call_builtin(
            ctx,
            YulFunction::AlignedStorageStore,
            vec!["e_offs".to_string(), linked_dst_name].into_iter(),
        );

        ctx.writer.unindent();
        emitln!(ctx.writer, "}");
    }

    // increment size
    emitln!(ctx.writer, "size := add(size, 1)");

    emitln!(
        ctx.writer,
        &gen.parent.call_builtin_str(
            ctx,
            YulFunction::StoreU64,
            vec!["v_ptr".to_string(), "size".to_string()].into_iter()
        )
    );

    // load capacity
    emitln!(
        ctx.writer,
        "let capacity := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::LoadU64,
            std::iter::once("$IndexPtr(v_ptr, 8)".to_string())
        )
    );

    // if in memory and size == capacity, resize
    emitln!(
        ctx.writer,
        "if and(iszero({}), eq(size, capacity)) {{",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::IsStoragePtr,
            std::iter::once("v_ptr".to_string())
        ),
    );

    ctx.writer.indent();

    emitln!(
        ctx.writer,
        "let new_v_offs := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::ResizeVector,
            vec![
                "v_offs".to_string(),
                "capacity".to_string(),
                elem_type_size.to_string()
            ]
            .into_iter()
        )
    );
    emitln!(
        ctx.writer,
        &gen.parent.call_builtin_str(
            ctx,
            YulFunction::StoreU256,
            vec!["v_ref".to_string(), "new_v_offs".to_string()].into_iter()
        )
    );
    ctx.writer.unindent();
    emitln!(ctx.writer, "}");
    ctx.writer.unindent();
    emitln!(ctx.writer, "}");
}

fn define_swap_fun(gen: &mut FunctionGenerator, ctx: &Context, fun_id: &QualifiedInstId<FunId>) {
    let elem_type = fun_id.inst.get(0).unwrap();
    let elem_type_size = ctx.type_size(elem_type);
    emitln!(ctx.writer, "(v_ref, i, j) {");
    ctx.writer.indent();
    emitln!(
        ctx.writer,
        "let v_offs := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::LoadU256,
            std::iter::once("v_ref".to_string())
        )
    );
    let is_storage_call = gen.parent.call_builtin_str(
        ctx,
        YulFunction::IsStoragePtr,
        std::iter::once("v_ref".to_string()),
    );
    emitln!(
        ctx.writer,
        "let v_ptr := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::MakePtr,
            vec![is_storage_call, "v_offs".to_string()].into_iter()
        )
    );
    emitln!(
        ctx.writer,
        "let size := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::LoadU64,
            std::iter::once("v_ptr".to_string())
        )
    );

    emitln!(
        ctx.writer,
        "if or({}, {}) {{ {} }}",
        &gen.parent.call_builtin_str(
            ctx,
            YulFunction::GtEq,
            vec!["i".to_string(), "size".to_string()].into_iter()
        ),
        &gen.parent.call_builtin_str(
            ctx,
            YulFunction::GtEq,
            vec!["j".to_string(), "size".to_string()].into_iter()
        ),
        &gen.parent
            .call_builtin_str(ctx, YulFunction::AbortBuiltin, std::iter::empty())
    );

    emitln!(
        ctx.writer,
        "let i_ptr := {}",
        &gen.parent.call_builtin_str(
            ctx,
            YulFunction::IndexPtr,
            vec![
                "v_ptr".to_string(),
                format!("add({}, mul(i, {}))", VECTOR_METADATA_SIZE, elem_type_size)
            ]
            .into_iter()
        )
    );
    emitln!(
        ctx.writer,
        "let j_ptr := {}",
        &gen.parent.call_builtin_str(
            ctx,
            YulFunction::IndexPtr,
            vec![
                "v_ptr".to_string(),
                format!("add({}, mul(j, {}))", VECTOR_METADATA_SIZE, elem_type_size)
            ]
            .into_iter()
        )
    );
    emitln!(
        ctx.writer,
        "let i_val := {}",
        &gen.parent.call_builtin_str(
            ctx,
            ctx.load_builtin_fun(elem_type),
            std::iter::once("i_ptr".to_string())
        )
    );
    emitln!(
        ctx.writer,
        "let j_val := {}",
        &gen.parent.call_builtin_str(
            ctx,
            ctx.load_builtin_fun(elem_type),
            std::iter::once("j_ptr".to_string())
        )
    );
    emitln!(
        ctx.writer,
        &gen.parent.call_builtin_str(
            ctx,
            ctx.store_builtin_fun(elem_type),
            vec!["i_ptr".to_string(), "j_val".to_string()].into_iter()
        )
    );
    emitln!(
        ctx.writer,
        &gen.parent.call_builtin_str(
            ctx,
            ctx.store_builtin_fun(elem_type),
            vec!["j_ptr".to_string(), "i_val".to_string()].into_iter()
        )
    );
    ctx.writer.unindent();
    emitln!(ctx.writer, "}");
}

fn define_destroy_empty_fun(
    gen: &mut FunctionGenerator,
    ctx: &Context,
    fun_id: &QualifiedInstId<FunId>,
) {
    assert_eq!(
        fun_id.inst.len(),
        1,
        "vector instantiated with non-one type parameter"
    );
    emitln!(ctx.writer, "(v) {");
    ctx.writer.indent();
    let type_size = ctx.type_size(fun_id.inst.get(0).unwrap());
    emitln!(
        ctx.writer,
        "let size := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::MemoryLoadU64,
            std::iter::once("v".to_string())
        )
    );

    // check that the vector is indeed empty

    emitln!(
        ctx.writer,
        "if {} {{ {} }}",
        &gen.parent.call_builtin_str(
            ctx,
            YulFunction::LogicalNot,
            std::iter::once("iszero(size)".to_string())
        ),
        &gen.parent
            .call_builtin_str(ctx, YulFunction::AbortBuiltin, std::iter::empty())
    );

    emitln!(
        ctx.writer,
        "let capacity := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::MemoryLoadU64,
            std::iter::once("add(v, 8)".to_string())
        )
    );

    emitln!(
        ctx.writer,
        &gen.parent.call_builtin_str(
            ctx,
            YulFunction::Free,
            vec![
                "v".to_string(),
                format!(
                    "add({}, mul(capacity, {}))",
                    VECTOR_METADATA_SIZE, type_size
                )
            ]
            .into_iter()
        )
    );

    ctx.writer.unindent();
    emitln!(ctx.writer, "}");
}

/// Generate equality method for the vector type.
pub(crate) fn equality_fun(gen: &mut Generator, ctx: &Context, ty: &Type) {
    let elem_type = get_elem_type(ty).unwrap();
    if ctx.type_allocates_memory(&elem_type) {
        emitln!(
            ctx.writer,
            "let len_x := {}",
            gen.call_builtin_str(
                ctx,
                YulFunction::MemoryLoadU64,
                std::iter::once("x".to_string())
            )
        );
        emitln!(
            ctx.writer,
            "let len_y := {}",
            gen.call_builtin_str(
                ctx,
                YulFunction::MemoryLoadU64,
                std::iter::once("y".to_string())
            )
        );
        emitln!(
            ctx.writer,
            "if {} {{\n  res:= false\n  leave\n}}",
            gen.call_builtin_str(
                ctx,
                YulFunction::Neq,
                vec!["len_x".to_string(), "len_y".to_string()].into_iter()
            )
        );
        emitln!(
            ctx.writer,
            "for { let i := 0 } lt(i, len_x) { i := add(i, 1) }"
        );
        let elem_size = ctx.type_size(&elem_type);
        ctx.emit_block(|| {
            emitln!(
                ctx.writer,
                "let e_x := {}",
                gen.call_builtin_str(
                    ctx,
                    ctx.memory_load_builtin_fun(&elem_type),
                    std::iter::once(format!(
                        "add({}, add(x, mul(i, {})))",
                        VECTOR_METADATA_SIZE, elem_size
                    ))
                )
            );
            emitln!(
                ctx.writer,
                "let e_y := {}",
                gen.call_builtin_str(
                    ctx,
                    ctx.memory_load_builtin_fun(&elem_type),
                    std::iter::once(format!(
                        "add({}, add(y, mul(i, {})))",
                        VECTOR_METADATA_SIZE, elem_size
                    ))
                )
            );
            let elem_equality_call = format!("{}(e_x, e_y)", gen.equality_function(ctx, elem_type));
            emitln!(
                ctx.writer,
                "if {} {{\n  res:= false\n  leave\n}}",
                gen.call_builtin_str(
                    ctx,
                    YulFunction::LogicalNot,
                    std::iter::once(elem_equality_call)
                )
            );
        });
        emitln!(ctx.writer, "res := true");
    } else {
        emitln!(
            ctx.writer,
            "res := {}",
            gen.call_builtin_str(
                ctx,
                YulFunction::EqVector,
                vec![
                    "x".to_string(),
                    "y".to_string(),
                    ctx.type_size(&elem_type).to_string()
                ]
                .into_iter()
            )
        );
    }
}

pub(crate) fn get_elem_type(vector_type: &Type) -> Option<Type> {
    match vector_type {
        Type::Vector(ty) => Some(*ty.clone()),
        _ => None,
    }
}
