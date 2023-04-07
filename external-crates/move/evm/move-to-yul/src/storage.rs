// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{context::Context, yul_functions::YulFunction, Generator};
use move_model::{
    emitln,
    model::{QualifiedInstId, StructId},
    ty::Type,
};

impl Generator {
    /// Move resource from memory to storage.
    pub(crate) fn move_to(
        &mut self,
        ctx: &Context,
        struct_id: QualifiedInstId<StructId>,
        signer_ref: String,
        value: String,
    ) {
        let addr = self.call_builtin_str(ctx, YulFunction::LoadU256, std::iter::once(signer_ref));
        self.move_to_addr(ctx, struct_id, addr, value)
    }

    /// Move resource from memory to storage, with direct address.
    pub(crate) fn move_to_addr(
        &mut self,
        ctx: &Context,
        struct_id: QualifiedInstId<StructId>,
        addr: String,
        value: String,
    ) {
        ctx.emit_block(|| {
            emitln!(
                ctx.writer,
                "let $base_offset := {}",
                self.type_storage_base(
                    ctx,
                    &struct_id.to_type(),
                    "${RESOURCE_STORAGE_CATEGORY}",
                    addr,
                )
            );
            let base_offset = "$base_offset";

            // At the base offset we store a boolean indicating whether the resource exists. Check this
            // and if it is set, abort. Otherwise set this bit.
            let exists_call = self.call_builtin_str(
                ctx,
                YulFunction::AlignedStorageLoad,
                std::iter::once(base_offset.to_string()),
            );
            let abort_call =
                self.call_builtin_str(ctx, YulFunction::AbortBuiltin, std::iter::empty());
            emitln!(ctx.writer, "if {} {{\n  {}\n}}", exists_call, abort_call);
            self.call_builtin(
                ctx,
                YulFunction::AlignedStorageStore,
                vec![base_offset.to_string(), "true".to_string()].into_iter(),
            );

            // Move the struct to storage.
            ctx.emit_block(|| {
                // The actual resource data starts at base_offset + 32. Set the destination address
                // to this.
                emitln!(
                    ctx.writer,
                    "let $dst := add({}, ${{RESOURCE_EXISTS_FLAG_SIZE}})",
                    base_offset
                );
                emitln!(ctx.writer, "let $src := {}", value);
                // Perform the move.
                self.move_struct_to_storage(
                    ctx,
                    &struct_id,
                    "$src".to_string(),
                    "$dst".to_string(),
                    true,
                );
            });
        })
    }

    /// Moves a struct from memory to storage. This recursively moves linked data like
    /// nested structs and vectors.
    pub(crate) fn move_struct_to_storage(
        &mut self,
        ctx: &Context,
        struct_id: &QualifiedInstId<StructId>,
        src: String,
        dst: String,
        clean_flag: bool,
    ) {
        let layout = ctx.get_struct_layout(struct_id);

        // By invariant we know that the leading fields are pointer fields. Copy them first.
        for field_offs in layout.field_order.iter().take(layout.pointer_count) {
            let (byte_offs, ty) = layout.offsets.get(field_offs).unwrap();
            assert_eq!(byte_offs % 32, 0, "pointer fields are on word boundary");
            ctx.emit_block(|| {
                let linked_src_name = format!("$linked_src_{}", self.type_hash(ctx, ty));
                let linked_dst_name = format!("$linked_dst_{}", self.type_hash(ctx, ty));

                // Load the pointer to the linked memory.
                emitln!(
                    ctx.writer,
                    "let {} := mload({})",
                    linked_src_name,
                    format!("add({}, {})", src, byte_offs)
                );
                self.create_and_move_data_to_linked_storage(
                    ctx,
                    ty,
                    linked_src_name,
                    linked_dst_name.clone(),
                    clean_flag,
                );
                // Store the result at the destination
                self.call_builtin(
                    ctx,
                    YulFunction::AlignedStorageStore,
                    vec![format!("add({}, {})", dst, byte_offs), linked_dst_name].into_iter(),
                )
            });
        }

        // The remaining fields are all primitive. We also know that memory is padded to word size,
        // so we can just copy directly word by word, which has the lowest gas cost.
        if layout.pointer_count < layout.field_order.len() {
            let mut byte_offs = layout
                .offsets
                .get(&layout.field_order[layout.pointer_count])
                .unwrap()
                .0;
            assert_eq!(
                byte_offs % 32,
                0,
                "first non-pointer field on word boundary"
            );
            while byte_offs < layout.size {
                self.call_builtin(
                    ctx,
                    YulFunction::AlignedStorageStore,
                    vec![
                        format!("add({}, {})", dst, byte_offs),
                        format!("mload(add({}, {}))", src, byte_offs),
                    ]
                    .into_iter(),
                );
                byte_offs += 32
            }
        }

        // Free the memory allocated by this struct.
        if clean_flag {
            self.call_builtin(
                ctx,
                YulFunction::Free,
                vec![src, layout.size.to_string()].into_iter(),
            )
        }
    }

    /// Move a struct from storage to memory, zeroing all associated storage. This recursively
    /// moves linked data like nested structs and vectors.
    pub(crate) fn move_struct_to_memory(
        &mut self,
        ctx: &Context,
        struct_id: &QualifiedInstId<StructId>,
        src: String,
        dst: String,
        clean_flag: bool, // whether to clean the storage
    ) {
        // Allocate struct.
        let layout = ctx.get_struct_layout(struct_id);
        emitln!(
            ctx.writer,
            "{} := {}",
            dst,
            self.call_builtin_str(
                ctx,
                YulFunction::Malloc,
                std::iter::once(layout.size.to_string()),
            )
        );

        // Copy fields. By invariant we know that the leading fields are pointer fields.
        for field_offs in layout.field_order.iter().take(layout.pointer_count) {
            let (byte_offs, ty) = layout.offsets.get(field_offs).unwrap();
            assert_eq!(byte_offs % 32, 0, "pointer fields are on word boundary");
            let field_src_ptr = format!("add({}, {})", src, byte_offs);
            let field_dst_ptr = format!("add({}, {})", dst, byte_offs);
            ctx.emit_block(|| {
                let hash = self.type_hash(ctx, ty);
                let linked_src_name = format!("$linked_src_{}", hash);
                let linked_dst_name = format!("$linked_dst_{}", hash);

                // Load the pointer to the linked storage.
                let load_call = self.call_builtin_str(
                    ctx,
                    YulFunction::AlignedStorageLoad,
                    std::iter::once(field_src_ptr.clone()),
                );

                emitln!(ctx.writer, "let {} := {}", linked_src_name, load_call);

                // Declare where to store the result and recursively move
                emitln!(ctx.writer, "let {}", linked_dst_name);
                self.move_data_from_linked_storage(
                    ctx,
                    ty,
                    linked_src_name,
                    linked_dst_name.clone(),
                    clean_flag,
                );
                // Store the result at the destination.
                emitln!(ctx.writer, "mstore({}, {})", field_dst_ptr, linked_dst_name);
                // Clear the storage to get a refund
                if clean_flag {
                    self.call_builtin(
                        ctx,
                        YulFunction::AlignedStorageStore,
                        vec![field_src_ptr, 0.to_string()].into_iter(),
                    );
                }
            });
        }

        // The remaining fields are all primitive. We also know that memory is padded to word size,
        // so we can just copy directly word by word, which has the lowest gas cost.
        if layout.pointer_count < layout.field_order.len() {
            let mut byte_offs = layout
                .offsets
                .get(&layout.field_order[layout.pointer_count])
                .unwrap()
                .0;
            assert_eq!(
                byte_offs % 32,
                0,
                "first non-pointer field on word boundary"
            );
            while byte_offs < layout.size {
                let field_src_ptr = format!("add({}, {})", src, byte_offs);
                let field_dst_ptr = format!("add({}, {})", dst, byte_offs);
                let load_call = self.call_builtin_str(
                    ctx,
                    YulFunction::AlignedStorageLoad,
                    std::iter::once(field_src_ptr.clone()),
                );
                emitln!(ctx.writer, "mstore({}, {})", field_dst_ptr, load_call);
                if clean_flag {
                    self.call_builtin(
                        ctx,
                        YulFunction::AlignedStorageStore,
                        vec![field_src_ptr, 0.to_string()].into_iter(),
                    );
                }
                byte_offs += 32
            }
        }
    }

    // Recursively move struct or vector data to corresponding linked storage.
    // This function calls `move_struct_to_storage` and `move_vector_to_storage`, and
    // is called by these two functions too.
    pub(crate) fn create_and_move_data_to_linked_storage(
        &mut self,
        ctx: &Context,
        ty: &Type,
        linked_src_name: String,
        linked_dst_name: String,
        clean_flag: bool,
    ) {
        let hash = self.type_hash(ctx, ty);
        // Allocate a new storage pointer.
        emitln!(
            ctx.writer,
            "let {} := {}",
            linked_dst_name,
            self.call_builtin_str(
                ctx,
                YulFunction::NewLinkedStorageBase,
                std::iter::once(format!("0x{:x}", hash))
            )
        );

        // Recursively move.
        if ty.is_vector() {
            self.move_vector_to_storage(ctx, ty, linked_src_name, linked_dst_name, clean_flag);
        } else if ctx.type_is_struct(ty) {
            let field_struct_id = ty.get_struct_id(ctx.env).expect("struct");
            self.move_struct_to_storage(
                ctx,
                &field_struct_id,
                linked_src_name,
                linked_dst_name,
                clean_flag,
            );
        } else {
            // Primitive type so directly store the src at the location
            self.call_builtin(
                ctx,
                ctx.storage_store_builtin_fun(ty),
                vec![linked_dst_name, linked_src_name].into_iter(),
            );
        }
    }

    // Recursively move struct or vector data from linked storage to memory.
    // This function calls `move_struct_to_memory` and `move_vector_to_memory`, and
    // is called by these two functions too.
    pub(crate) fn move_data_from_linked_storage(
        &mut self,
        ctx: &Context,
        ty: &Type,
        linked_src_name: String,
        linked_dst_name: String,
        clean_flag: bool,
    ) {
        if ty.is_vector() {
            self.move_vector_to_memory(ctx, ty, linked_src_name, linked_dst_name, clean_flag);
        } else if ctx.type_is_struct(ty) {
            let field_struct_id = ty.get_struct_id(ctx.env).expect("struct");
            self.move_struct_to_memory(
                ctx,
                &field_struct_id,
                linked_src_name,
                linked_dst_name,
                clean_flag,
            );
        } else {
            // Primitive type
            emitln!(
                ctx.writer,
                "{} := {}",
                linked_dst_name,
                self.call_builtin_str(
                    ctx,
                    ctx.storage_load_builtin_fun(ty),
                    std::iter::once(linked_src_name)
                )
            );
        }
    }

    /// Generate instructions for a borrow_global and return a String which denotes an
    /// expression valid in the current block. Calls to this function should usually
    /// scoped into a `ctx.emit_block`.
    pub(crate) fn borrow_global_instrs(
        &mut self,
        ctx: &Context,
        struct_id: &QualifiedInstId<StructId>,
        addr: String,
    ) -> String {
        // Obtain the storage base offset for this resource.
        emitln!(
            ctx.writer,
            "let $base_offset := {}",
            self.type_storage_base(
                ctx,
                &struct_id.to_type(),
                "${RESOURCE_STORAGE_CATEGORY}",
                addr,
            )
        );
        let base_offset = "$base_offset";

        // At the base offset check the flag whether the resource exists.
        let exists_call = self.call_builtin_str(
            ctx,
            YulFunction::AlignedStorageLoad,
            std::iter::once(base_offset.to_string()),
        );
        let abort_call = self.call_builtin_str(ctx, YulFunction::AbortBuiltin, std::iter::empty());
        emitln!(
            ctx.writer,
            "if iszero({}) {{\n  {}\n}}",
            exists_call,
            abort_call
        );

        // Skip the existence flag and create a pointer.

        self.call_builtin_str(
            ctx,
            YulFunction::MakePtr,
            vec![
                "true".to_string(),
                format!("add({}, ${{RESOURCE_EXISTS_FLAG_SIZE}})", base_offset),
            ]
            .into_iter(),
        )
    }

    /// Returns an expression for checking whether a resource exists.
    pub(crate) fn exists_check(
        &mut self,
        ctx: &Context,
        struct_id: QualifiedInstId<StructId>,
        addr: String,
    ) -> String {
        // Obtain the storage base offset for this resource.
        let base_offset = self.type_storage_base(
            ctx,
            &struct_id.to_type(),
            "${RESOURCE_STORAGE_CATEGORY}",
            addr,
        );
        // Load the exists flag and store it into destination.
        self.call_builtin_str(
            ctx,
            YulFunction::AlignedStorageLoad,
            std::iter::once(base_offset),
        )
    }
}
