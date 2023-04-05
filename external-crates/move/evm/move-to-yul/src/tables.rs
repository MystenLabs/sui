// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// This file defines functionalities regarding tables.

use crate::{
    context::Context, functions::FunctionGenerator, native_functions::NativeFunctions,
    yul_functions::YulFunction,
};
use move_model::{
    emitln,
    model::{FunId, QualifiedInstId},
    ty::TypeDisplayContext,
};

impl NativeFunctions {
    /// Define table functions for a specific instantiation.
    pub(crate) fn define_table_functions(&mut self, ctx: &Context) {
        let table = &self.find_module(ctx, "0x2", "Table");

        self.define(ctx, table, "empty", define_empty_fun);
        self.define(ctx, table, "contains", define_contains_fun);
        self.define(ctx, table, "insert", define_insert_fun);
        self.define(ctx, table, "borrow", define_borrow_fun);
        self.define(ctx, table, "borrow_mut", define_borrow_fun);
        self.define(ctx, table, "remove", define_remove_fun);
    }
}

fn define_empty_fun(gen: &mut FunctionGenerator, ctx: &Context, fun_id: &QualifiedInstId<FunId>) {
    let key_type = fun_id.inst.get(0).expect("key type");

    // TODO: right now the key to storage is simply hash (table_handle, key), which works when key
    // is a primitive type. However, this doesn't work when key is a struct or a vector, represented
    // by a pointer to its data. We need to implement a hash algorithm for structs and vectors before
    // allowing using structs and vectors as table keys.
    if ctx.type_allocates_memory(key_type) {
        let ty_ctx = &TypeDisplayContext::WithEnv {
            env: ctx.env,
            type_param_names: None,
        };
        ctx.env.error(
            &gen.parent.contract_loc,
            &format!(
                "Type `{}` is not supported currently as table key. Only primitive types are supported.",
                key_type.display(ty_ctx),
            ),
        )
    }

    emitln!(ctx.writer, "() -> table {");
    ctx.writer.indent();
    emitln!(
        ctx.writer,
        "table := {}",
        gen.parent
            .call_builtin_str(ctx, YulFunction::NewTableHandle, std::iter::empty(),)
    );
    ctx.writer.unindent();
    emitln!(ctx.writer, "}");
}

fn define_contains_fun(
    gen: &mut FunctionGenerator,
    ctx: &Context,
    fun_id: &QualifiedInstId<FunId>,
) {
    let key_type = fun_id.inst.get(0).expect("key type");

    emitln!(ctx.writer, "(table_ref, key_ref) -> res {");
    ctx.writer.indent();

    // get key from key_ref
    if ctx.type_is_struct(key_type) {
        emitln!(
            ctx.writer,
            "let key := {}",
            gen.parent.call_builtin_str(
                ctx,
                YulFunction::OffsetPtr,
                std::iter::once("key_ref".to_string()),
            )
        );
    } else {
        emitln!(
            ctx.writer,
            "let key := {}",
            gen.parent.call_builtin_str(
                ctx,
                ctx.load_builtin_fun(key_type),
                std::iter::once("key_ref".to_string()),
            )
        );
    }

    // get the table handle from table_ref
    emitln!(
        ctx.writer,
        "let table_handle := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::LoadU256,
            std::iter::once("table_ref".to_string()),
        )
    );

    // create a new storage key with keccak(table handle + key)
    emitln!(
        ctx.writer,
        "let storage_key := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::StorageKey,
            vec!["table_handle".to_string(), "key".to_string()].into_iter(),
        )
    );

    emitln!(ctx.writer, "let word := sload(storage_key)");

    emitln!(
        ctx.writer,
        "res := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::LogicalNot,
            std::iter::once("iszero(word)".to_string())
        )
    );
    // emitln!(ctx.writer, "res := word");
    ctx.writer.unindent();
    emitln!(ctx.writer, "}");
}

fn define_insert_fun(gen: &mut FunctionGenerator, ctx: &Context, fun_id: &QualifiedInstId<FunId>) {
    let key_type = fun_id.inst.get(0).expect("key type");
    let value_type = fun_id.inst.get(1).expect("value type");

    emitln!(ctx.writer, "(table_ref, key_ref, value) {");
    ctx.writer.indent();

    // get key from key_ref
    if ctx.type_is_struct(key_type) {
        emitln!(
            ctx.writer,
            "let key := {}",
            gen.parent.call_builtin_str(
                ctx,
                YulFunction::OffsetPtr,
                std::iter::once("key_ref".to_string()),
            )
        );
    } else {
        emitln!(
            ctx.writer,
            "let key := {}",
            gen.parent.call_builtin_str(
                ctx,
                ctx.load_builtin_fun(key_type),
                std::iter::once("key_ref".to_string()),
            )
        );
    }

    // get the table handle from table_ref
    emitln!(
        ctx.writer,
        "let table_handle := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::LoadU256,
            std::iter::once("table_ref".to_string()),
        )
    );

    // create a new storage key with keccak(table handle + key)
    emitln!(
        ctx.writer,
        "let storage_key := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::StorageKey,
            vec!["table_handle".to_string(), "key".to_string()].into_iter(),
        )
    );

    emitln!(ctx.writer, "let word := sload(storage_key)");

    // abort if the spot is taken
    emitln!(
        ctx.writer,
        "if {} {{\n  {}\n}}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::LogicalNot,
            std::iter::once("iszero(word)".to_string())
        ),
        gen.parent
            .call_builtin_str(ctx, YulFunction::AbortBuiltin, std::iter::empty())
    );

    let hash = gen.parent.type_hash(ctx, value_type);
    let linked_dst_name = format!("$linked_dst_{}", hash);

    gen.parent.create_and_move_data_to_linked_storage(
        ctx,
        value_type,
        "value".to_string(),
        linked_dst_name.clone(),
        true,
    );

    if value_type.is_vector() {
        // create another layer of ptr
        emitln!(
            ctx.writer,
            "let vector_ref_dst := {}",
            gen.parent.call_builtin_str(
                ctx,
                YulFunction::NewLinkedStorageBase,
                std::iter::once(format!("0x{:x}", hash))
            )
        );
        gen.parent.call_builtin(
            ctx,
            YulFunction::AlignedStorageStore,
            vec!["vector_ref_dst".to_string(), linked_dst_name].into_iter(),
        );
        emitln!(ctx.writer, "sstore(storage_key, vector_ref_dst)");
    } else {
        emitln!(ctx.writer, "sstore(storage_key, {})", linked_dst_name);
    }

    ctx.writer.unindent();
    emitln!(ctx.writer, "}");
}

fn define_borrow_fun(gen: &mut FunctionGenerator, ctx: &Context, fun_id: &QualifiedInstId<FunId>) {
    let key_type = fun_id.inst.get(0).expect("key type");

    emitln!(ctx.writer, "(table_ref, key_ref) -> value_ref {");
    ctx.writer.indent();

    // get key from key_ref
    if ctx.type_is_struct(key_type) {
        emitln!(
            ctx.writer,
            "let key := {}",
            gen.parent.call_builtin_str(
                ctx,
                YulFunction::OffsetPtr,
                std::iter::once("key_ref".to_string()),
            )
        );
    } else {
        emitln!(
            ctx.writer,
            "let key := {}",
            gen.parent.call_builtin_str(
                ctx,
                ctx.load_builtin_fun(key_type),
                std::iter::once("key_ref".to_string()),
            )
        );
    }

    // get the table handle from table_ref
    emitln!(
        ctx.writer,
        "let table_handle := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::LoadU256,
            std::iter::once("table_ref".to_string()),
        )
    );

    // create a new storage key with keccak(table handle + key)
    emitln!(
        ctx.writer,
        "let storage_key := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::StorageKey,
            vec!["table_handle".to_string(), "key".to_string()].into_iter(),
        )
    );

    emitln!(ctx.writer, "let value_offs := sload(storage_key)");

    // abort if entry does not exist
    emitln!(
        ctx.writer,
        "if iszero(value_offs) {{\n  {}\n}}",
        gen.parent
            .call_builtin_str(ctx, YulFunction::AbortBuiltin, std::iter::empty())
    );

    emitln!(
        ctx.writer,
        "value_ref := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::MakePtr,
            vec![true.to_string(), "value_offs".to_string()].into_iter()
        )
    );

    ctx.writer.unindent();
    emitln!(ctx.writer, "}");
}

fn define_remove_fun(gen: &mut FunctionGenerator, ctx: &Context, fun_id: &QualifiedInstId<FunId>) {
    let key_type = fun_id.inst.get(0).expect("key type");
    let value_type = fun_id.inst.get(1).expect("value type");

    emitln!(ctx.writer, "(table_ref, key_ref) -> value {");
    ctx.writer.indent();

    // get key from key_ref
    if ctx.type_is_struct(key_type) {
        emitln!(
            ctx.writer,
            "let key := {}",
            gen.parent.call_builtin_str(
                ctx,
                YulFunction::OffsetPtr,
                std::iter::once("key_ref".to_string()),
            )
        );
    } else {
        emitln!(
            ctx.writer,
            "let key := {}",
            gen.parent.call_builtin_str(
                ctx,
                ctx.load_builtin_fun(key_type),
                std::iter::once("key_ref".to_string()),
            )
        );
    }

    // get the table handle from table_ref
    emitln!(
        ctx.writer,
        "let table_handle := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::LoadU256,
            std::iter::once("table_ref".to_string()),
        )
    );

    // create a new storage key with keccak(table handle + key)
    emitln!(
        ctx.writer,
        "let storage_key := {}",
        gen.parent.call_builtin_str(
            ctx,
            YulFunction::StorageKey,
            vec!["table_handle".to_string(), "key".to_string()].into_iter(),
        )
    );

    emitln!(ctx.writer, "let linked_src := sload(storage_key)");

    // abort if the entry does not exist
    emitln!(
        ctx.writer,
        "if iszero(linked_src) {{\n  {}\n}}",
        gen.parent
            .call_builtin_str(ctx, YulFunction::AbortBuiltin, std::iter::empty())
    );

    if value_type.is_vector() {
        emitln!(
            ctx.writer,
            "let vector_linked_src := {}",
            gen.parent.call_builtin_str(
                ctx,
                YulFunction::StorageLoadU256,
                std::iter::once("linked_src".to_string())
            )
        );

        // get refund for vector linked src
        gen.parent.call_builtin(
            ctx,
            YulFunction::AlignedStorageStore,
            vec!["linked_src".to_string(), "0".to_string()].into_iter(),
        );

        emitln!(ctx.writer, "linked_src := vector_linked_src");
    }

    gen.parent.move_data_from_linked_storage(
        ctx,
        value_type,
        "linked_src".to_string(),
        "value".to_string(),
        true,
    );

    gen.parent.call_builtin(
        ctx,
        YulFunction::AlignedStorageStore,
        vec!["linked_src".to_string(), "0".to_string()].into_iter(),
    );

    emitln!(ctx.writer, "sstore(storage_key, 0)");

    ctx.writer.unindent();
    emitln!(ctx.writer, "}");
}
