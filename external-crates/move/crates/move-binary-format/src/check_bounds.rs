// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    errors::{
        bounds_error, offset_out_of_bounds as offset_out_of_bounds_error, verification_error,
        PartialVMError, PartialVMResult,
    },
    file_format::{
        AbilitySet, Bytecode, CodeOffset, CodeUnit, CompiledModule, Constant, DatatypeHandle,
        EnumDefInstantiation, EnumDefinition, FieldHandle, FieldInstantiation, FunctionDefinition,
        FunctionDefinitionIndex, FunctionHandle, FunctionInstantiation, JumpTableInner, LocalIndex,
        ModuleHandle, Signature, SignatureToken, StructDefInstantiation, StructDefinition,
        StructFieldInformation, TableIndex, VariantDefinition, VariantHandle,
        VariantInstantiationHandle, VariantJumpTable,
    },
    internals::ModuleIndex,
    IndexKind,
};
use move_core_types::vm_status::StatusCode;

enum BoundsCheckingContext {
    Module,
    ModuleFunction(FunctionDefinitionIndex),
}
pub struct BoundsChecker<'a> {
    module: &'a CompiledModule,
    context: BoundsCheckingContext,
}

impl<'a> BoundsChecker<'a> {
    pub fn verify_module(module: &'a CompiledModule) -> PartialVMResult<()> {
        let mut bounds_check = Self {
            module,
            context: BoundsCheckingContext::Module,
        };
        if bounds_check.module.module_handles().is_empty() {
            let status =
                verification_error(StatusCode::NO_MODULE_HANDLES, IndexKind::ModuleHandle, 0);
            return Err(status);
        }
        bounds_check.verify_impl()
    }

    fn verify_impl(&mut self) -> PartialVMResult<()> {
        self.check_signatures()?;
        self.check_constants()?;
        self.check_module_handles()?;
        self.check_self_module_handle()?;
        self.check_datatype_handles()?;
        self.check_function_handles()?;
        self.check_field_handles()?;
        self.check_friend_decls()?;
        self.check_struct_instantiations()?;
        self.check_function_instantiations()?;
        self.check_field_instantiations()?;
        self.check_struct_defs()?;
        self.check_enum_defs()?;
        self.check_enum_instantiations()?;
        // NB: the order of these checks is important and must occur after the enum checks, and
        // before the function checks.
        self.check_variant_handles()?;
        self.check_variant_instantiation_handles()?;
        self.check_function_defs()?;
        Ok(())
    }

    fn check_signatures(&self) -> PartialVMResult<()> {
        for signature in self.module.signatures() {
            self.check_signature(signature)?
        }
        Ok(())
    }

    fn check_constants(&self) -> PartialVMResult<()> {
        for constant in self.module.constant_pool() {
            self.check_constant(constant)?
        }
        Ok(())
    }

    fn check_module_handles(&self) -> PartialVMResult<()> {
        for script_handle in self.module.module_handles() {
            self.check_module_handle(script_handle)?
        }
        Ok(())
    }

    fn check_datatype_handles(&self) -> PartialVMResult<()> {
        for struct_handle in self.module.datatype_handles() {
            self.check_datatype_handle(struct_handle)?
        }
        Ok(())
    }

    fn check_function_handles(&self) -> PartialVMResult<()> {
        for function_handle in self.module.function_handles() {
            self.check_function_handle(function_handle)?
        }
        Ok(())
    }

    fn check_field_handles(&self) -> PartialVMResult<()> {
        for field_handle in self.module.field_handles() {
            self.check_field_handle(field_handle)?
        }
        Ok(())
    }

    fn check_friend_decls(&self) -> PartialVMResult<()> {
        for friend_decl in self.module.friend_decls() {
            self.check_module_handle(friend_decl)?
        }
        Ok(())
    }

    fn check_struct_instantiations(&self) -> PartialVMResult<()> {
        for struct_instantiation in self.module.struct_instantiations() {
            self.check_struct_instantiation(struct_instantiation)?
        }
        Ok(())
    }

    fn check_enum_instantiations(&self) -> PartialVMResult<()> {
        for enum_instantiation in self.module.enum_instantiations() {
            self.check_enum_instantiation(enum_instantiation)?
        }
        Ok(())
    }

    fn check_variant_handles(&self) -> PartialVMResult<()> {
        for variant_handle in self.module.variant_handles() {
            self.check_variant_handle(variant_handle)?
        }
        Ok(())
    }

    fn check_variant_instantiation_handles(&self) -> PartialVMResult<()> {
        for variant_instantiation_handle in self.module.variant_instantiation_handles() {
            self.check_variant_instantiation_handle(variant_instantiation_handle)?
        }
        Ok(())
    }

    fn check_function_instantiations(&self) -> PartialVMResult<()> {
        for function_instantiation in self.module.function_instantiations() {
            self.check_function_instantiation(function_instantiation)?
        }
        Ok(())
    }

    fn check_field_instantiations(&self) -> PartialVMResult<()> {
        for field_instantiation in self.module.field_instantiations() {
            self.check_field_instantiation(field_instantiation)?
        }
        Ok(())
    }

    fn check_struct_defs(&self) -> PartialVMResult<()> {
        for struct_def in self.module.struct_defs() {
            self.check_struct_def(struct_def)?
        }
        Ok(())
    }

    fn check_enum_defs(&self) -> PartialVMResult<()> {
        for enum_def in self.module.enum_defs() {
            self.check_enum_def(enum_def)?
        }
        Ok(())
    }

    fn check_function_defs(&mut self) -> PartialVMResult<()> {
        for (function_def_idx, function_def) in self.module.function_defs().iter().enumerate() {
            self.check_function_def(function_def_idx, function_def)?
        }
        Ok(())
    }

    fn check_module_handle(&self, module_handle: &ModuleHandle) -> PartialVMResult<()> {
        check_bounds_impl(self.module.address_identifiers(), module_handle.address)?;
        check_bounds_impl(self.module.identifiers(), module_handle.name)
    }

    fn check_self_module_handle(&self) -> PartialVMResult<()> {
        check_bounds_impl(self.module.module_handles(), self.module.self_handle_idx())
    }

    fn check_datatype_handle(&self, datatype_handle: &DatatypeHandle) -> PartialVMResult<()> {
        check_bounds_impl(self.module.module_handles(), datatype_handle.module)?;
        check_bounds_impl(self.module.identifiers(), datatype_handle.name)
    }

    fn check_function_handle(&self, function_handle: &FunctionHandle) -> PartialVMResult<()> {
        check_bounds_impl(self.module.module_handles(), function_handle.module)?;
        check_bounds_impl(self.module.identifiers(), function_handle.name)?;
        check_bounds_impl(self.module.signatures(), function_handle.parameters)?;
        check_bounds_impl(self.module.signatures(), function_handle.return_)?;
        // function signature type paramters must be in bounds to the function type parameters
        let type_param_count = function_handle.type_parameters.len();
        if let Some(sig) = self
            .module
            .signatures()
            .get(function_handle.parameters.into_index())
        {
            for ty in &sig.0 {
                self.check_type_parameter(ty, type_param_count)?
            }
        }
        if let Some(sig) = self
            .module
            .signatures()
            .get(function_handle.return_.into_index())
        {
            for ty in &sig.0 {
                self.check_type_parameter(ty, type_param_count)?
            }
        }
        Ok(())
    }

    fn check_field_handle(&self, field_handle: &FieldHandle) -> PartialVMResult<()> {
        check_bounds_impl(self.module.struct_defs(), field_handle.owner)?;
        // field offset must be in bounds, struct def just checked above must exist
        if let Some(struct_def) = &self
            .module
            .struct_defs()
            .get(field_handle.owner.into_index())
        {
            let fields_count = match &struct_def.field_information {
                StructFieldInformation::Native => 0,
                StructFieldInformation::Declared(fields) => fields.len(),
            };
            if field_handle.field as usize >= fields_count {
                return Err(bounds_error(
                    StatusCode::INDEX_OUT_OF_BOUNDS,
                    IndexKind::MemberCount,
                    field_handle.field,
                    fields_count,
                ));
            }
        }
        Ok(())
    }

    fn check_struct_instantiation(
        &self,
        struct_instantiation: &StructDefInstantiation,
    ) -> PartialVMResult<()> {
        check_bounds_impl(self.module.struct_defs(), struct_instantiation.def)?;
        check_bounds_impl(
            self.module.signatures(),
            struct_instantiation.type_parameters,
        )
    }

    fn check_enum_instantiation(
        &self,
        enum_instantiation: &EnumDefInstantiation,
    ) -> PartialVMResult<()> {
        check_bounds_impl(self.module.enum_defs(), enum_instantiation.def)?;
        check_bounds_impl(self.module.signatures(), enum_instantiation.type_parameters)
    }

    fn check_variant_handle(&self, variant_handle: &VariantHandle) -> PartialVMResult<()> {
        check_bounds_impl(self.module.enum_defs(), variant_handle.enum_def)?;
        let enum_def = self.module.enum_def_at(variant_handle.enum_def);
        if variant_handle.variant as usize >= enum_def.variants.len() {
            return Err(bounds_error(
                StatusCode::INDEX_OUT_OF_BOUNDS,
                IndexKind::VariantTag,
                variant_handle.variant,
                enum_def.variants.len(),
            ));
        }
        Ok(())
    }

    fn check_variant_instantiation_handle(
        &self,
        variant_instantiation_handle: &VariantInstantiationHandle,
    ) -> PartialVMResult<()> {
        check_bounds_impl(
            self.module.enum_instantiations(),
            variant_instantiation_handle.enum_def,
        )?;
        let EnumDefInstantiation {
            def,
            type_parameters,
        } = self
            .module
            .enum_instantiation_at(variant_instantiation_handle.enum_def);
        // Invariant: enum instantiations have already been checked at this point.
        debug_assert!(check_bounds_impl(self.module.enum_defs(), *def).is_ok());
        debug_assert!(check_bounds_impl(self.module.signatures(), *type_parameters).is_ok());
        let enum_def = self.module.enum_def_at(*def);
        if variant_instantiation_handle.variant as usize >= enum_def.variants.len() {
            return Err(bounds_error(
                StatusCode::INDEX_OUT_OF_BOUNDS,
                IndexKind::VariantTag,
                variant_instantiation_handle.variant,
                enum_def.variants.len(),
            ));
        }
        Ok(())
    }

    fn check_function_instantiation(
        &self,
        function_instantiation: &FunctionInstantiation,
    ) -> PartialVMResult<()> {
        check_bounds_impl(
            self.module.function_handles(),
            function_instantiation.handle,
        )?;
        check_bounds_impl(
            self.module.signatures(),
            function_instantiation.type_parameters,
        )
    }

    fn check_field_instantiation(
        &self,
        field_instantiation: &FieldInstantiation,
    ) -> PartialVMResult<()> {
        check_bounds_impl(self.module.field_handles(), field_instantiation.handle)?;
        check_bounds_impl(
            self.module.signatures(),
            field_instantiation.type_parameters,
        )
    }

    fn check_signature(&self, signature: &Signature) -> PartialVMResult<()> {
        for ty in &signature.0 {
            self.check_type(ty)?
        }
        Ok(())
    }

    fn check_constant(&self, constant: &Constant) -> PartialVMResult<()> {
        self.check_type(&constant.type_)
    }

    fn check_struct_def(&self, struct_def: &StructDefinition) -> PartialVMResult<()> {
        check_bounds_impl(self.module.datatype_handles(), struct_def.struct_handle)?;
        // check signature (type) and type parameter for the field type
        if let StructFieldInformation::Declared(fields) = &struct_def.field_information {
            let type_param_count = self
                .module
                .datatype_handles()
                .get(struct_def.struct_handle.into_index())
                .map_or(0, |sh| sh.type_parameters.len());
            // field signatures are inlined
            for field in fields {
                check_bounds_impl(self.module.identifiers(), field.name)?;
                self.check_type(&field.signature.0)?;
                self.check_type_parameter(&field.signature.0, type_param_count)?;
            }
        }
        Ok(())
    }

    fn check_enum_def(&self, enum_def: &EnumDefinition) -> PartialVMResult<()> {
        check_bounds_impl(self.module.datatype_handles(), enum_def.enum_handle)?;
        let type_param_count = self
            .module
            .datatype_handles()
            .get(enum_def.enum_handle.into_index())
            .map_or(0, |eh| eh.type_parameters.len());
        for VariantDefinition {
            variant_name,
            fields,
        } in &enum_def.variants
        {
            check_bounds_impl(self.module.identifiers(), *variant_name)?;
            for field in fields {
                check_bounds_impl(self.module.identifiers(), field.name)?;
                self.check_type(&field.signature.0)?;
                self.check_type_parameter(&field.signature.0, type_param_count)?;
            }
        }
        Ok(())
    }

    fn check_function_def(
        &mut self,
        function_def_idx: usize,
        function_def: &FunctionDefinition,
    ) -> PartialVMResult<()> {
        self.context = BoundsCheckingContext::ModuleFunction(FunctionDefinitionIndex(
            function_def_idx as TableIndex,
        ));
        check_bounds_impl(self.module.function_handles(), function_def.function)?;
        for ty in &function_def.acquires_global_resources {
            check_bounds_impl(self.module.struct_defs(), *ty)?;
        }

        let code_unit = match &function_def.code {
            Some(code) => code,
            None => return Ok(()),
        };

        if function_def.function.into_index() >= self.module.function_handles().len() {
            return Err(verification_error(
                StatusCode::INDEX_OUT_OF_BOUNDS,
                IndexKind::FunctionDefinition,
                function_def_idx as TableIndex,
            ));
        }
        let function_handle = &self.module.function_handles()[function_def.function.into_index()];
        if function_handle.parameters.into_index() >= self.module.signatures().len() {
            return Err(verification_error(
                StatusCode::INDEX_OUT_OF_BOUNDS,
                IndexKind::FunctionDefinition,
                function_def_idx as TableIndex,
            ));
        }
        let parameters = &self.module.signatures()[function_handle.parameters.into_index()];

        self.check_code(
            code_unit,
            &function_handle.type_parameters,
            parameters,
            function_def_idx,
        )
    }

    fn check_code(
        &self,
        code_unit: &CodeUnit,
        type_parameters: &[AbilitySet],
        parameters: &Signature,
        index: usize,
    ) -> PartialVMResult<()> {
        check_bounds_impl(self.module.signatures(), code_unit.locals)?;

        let locals = self.get_locals(code_unit)?;
        // Use saturating add for stability
        let locals_count = locals.len().saturating_add(parameters.len());

        if locals_count > LocalIndex::MAX as usize {
            return Err(verification_error(
                StatusCode::TOO_MANY_LOCALS,
                IndexKind::FunctionDefinition,
                index as TableIndex,
            ));
        }

        // if there are locals check that the type parameters in local signature are in bounds.
        let type_param_count = type_parameters.len();
        for local in locals {
            self.check_type_parameter(local, type_param_count)?
        }

        // check bytecodes
        let code_len = code_unit.code.len();
        for (bytecode_offset, bytecode) in code_unit.code.iter().enumerate() {
            use self::Bytecode::*;

            match bytecode {
                LdConst(idx) => self.check_code_unit_bounds_impl(
                    self.module.constant_pool(),
                    *idx,
                    bytecode_offset,
                )?,
                MutBorrowField(idx) | ImmBorrowField(idx) => self.check_code_unit_bounds_impl(
                    self.module.field_handles(),
                    *idx,
                    bytecode_offset,
                )?,
                MutBorrowFieldGeneric(idx) | ImmBorrowFieldGeneric(idx) => {
                    self.check_code_unit_bounds_impl(
                        self.module.field_instantiations(),
                        *idx,
                        bytecode_offset,
                    )?;
                    // check type parameters in borrow are bound to the function type parameters
                    if let Some(field_inst) =
                        self.module.field_instantiations().get(idx.into_index())
                    {
                        if let Some(sig) = self
                            .module
                            .signatures()
                            .get(field_inst.type_parameters.into_index())
                        {
                            for ty in &sig.0 {
                                self.check_type_parameter(ty, type_param_count)?
                            }
                        }
                    }
                }
                Call(idx) => self.check_code_unit_bounds_impl(
                    self.module.function_handles(),
                    *idx,
                    bytecode_offset,
                )?,
                CallGeneric(idx) => {
                    self.check_code_unit_bounds_impl(
                        self.module.function_instantiations(),
                        *idx,
                        bytecode_offset,
                    )?;
                    // check type parameters in call are bound to the function type parameters
                    if let Some(func_inst) =
                        self.module.function_instantiations().get(idx.into_index())
                    {
                        if let Some(sig) = self
                            .module
                            .signatures()
                            .get(func_inst.type_parameters.into_index())
                        {
                            for ty in &sig.0 {
                                self.check_type_parameter(ty, type_param_count)?
                            }
                        }
                    }
                }
                Pack(idx)
                | Unpack(idx)
                | ExistsDeprecated(idx)
                | ImmBorrowGlobalDeprecated(idx)
                | MutBorrowGlobalDeprecated(idx)
                | MoveFromDeprecated(idx)
                | MoveToDeprecated(idx) => self.check_code_unit_bounds_impl(
                    self.module.struct_defs(),
                    *idx,
                    bytecode_offset,
                )?,
                PackGeneric(idx)
                | UnpackGeneric(idx)
                | ExistsGenericDeprecated(idx)
                | ImmBorrowGlobalGenericDeprecated(idx)
                | MutBorrowGlobalGenericDeprecated(idx)
                | MoveFromGenericDeprecated(idx)
                | MoveToGenericDeprecated(idx) => {
                    self.check_code_unit_bounds_impl(
                        self.module.struct_instantiations(),
                        *idx,
                        bytecode_offset,
                    )?;
                    // check type parameters in type operations are bound to the function type parameters
                    if let Some(struct_inst) =
                        self.module.struct_instantiations().get(idx.into_index())
                    {
                        if let Some(sig) = self
                            .module
                            .signatures()
                            .get(struct_inst.type_parameters.into_index())
                        {
                            for ty in &sig.0 {
                                self.check_type_parameter(ty, type_param_count)?
                            }
                        }
                    }
                }
                // Instructions that refer to this code block.
                BrTrue(offset) | BrFalse(offset) | Branch(offset) => {
                    let offset = *offset as usize;
                    if offset >= code_len {
                        return Err(self.offset_out_of_bounds(
                            StatusCode::INDEX_OUT_OF_BOUNDS,
                            IndexKind::CodeDefinition,
                            offset,
                            code_len,
                            bytecode_offset as CodeOffset,
                        ));
                    }
                }
                // Instructions that refer to the locals.
                CopyLoc(idx) | MoveLoc(idx) | StLoc(idx) | MutBorrowLoc(idx)
                | ImmBorrowLoc(idx) => {
                    let idx = *idx as usize;
                    if idx >= locals_count {
                        return Err(self.offset_out_of_bounds(
                            StatusCode::INDEX_OUT_OF_BOUNDS,
                            IndexKind::LocalPool,
                            idx,
                            locals_count,
                            bytecode_offset as CodeOffset,
                        ));
                    }
                }

                // Instructions that refer to a signature
                VecPack(idx, _)
                | VecLen(idx)
                | VecImmBorrow(idx)
                | VecMutBorrow(idx)
                | VecPushBack(idx)
                | VecPopBack(idx)
                | VecUnpack(idx, _)
                | VecSwap(idx) => {
                    self.check_code_unit_bounds_impl(
                        self.module.signatures(),
                        *idx,
                        bytecode_offset,
                    )?;
                    if let Some(sig) = self.module.signatures().get(idx.into_index()) {
                        for ty in &sig.0 {
                            self.check_type_parameter(ty, type_param_count)?;
                        }
                    }
                }

                // List out the other options explicitly so there's a compile error if a new
                // bytecode gets added.
                FreezeRef | Pop | Ret | LdU8(_) | LdU16(_) | LdU32(_) | LdU64(_) | LdU256(_)
                | LdU128(_) | CastU8 | CastU16 | CastU32 | CastU64 | CastU128 | CastU256
                | LdTrue | LdFalse | ReadRef | WriteRef | Add | Sub | Mul | Mod | Div | BitOr
                | BitAnd | Xor | Shl | Shr | Or | And | Not | Eq | Neq | Lt | Gt | Le | Ge
                | Abort | Nop => (),
                PackVariant(v_handle)
                | UnpackVariant(v_handle)
                | UnpackVariantImmRef(v_handle)
                | UnpackVariantMutRef(v_handle) => {
                    self.check_code_unit_bounds_impl(
                        self.module.variant_handles(),
                        *v_handle,
                        bytecode_offset,
                    )?;
                }
                PackVariantGeneric(vi_handle)
                | UnpackVariantGeneric(vi_handle)
                | UnpackVariantGenericImmRef(vi_handle)
                | UnpackVariantGenericMutRef(vi_handle) => {
                    self.check_code_unit_bounds_impl(
                        self.module.variant_instantiation_handles(),
                        *vi_handle,
                        bytecode_offset,
                    )?;
                    // Invariant: pool indices have already been checked at this point.
                    let handle = self.module.variant_instantiation_handle_at(*vi_handle);
                    let enum_inst = self.module.enum_instantiation_at(handle.enum_def);
                    let sig = self.module.signature_at(enum_inst.type_parameters);
                    for ty in &sig.0 {
                        self.check_type_parameter(ty, type_param_count)?
                    }
                }
                VariantSwitch(jti) => {
                    self.check_code_unit_bounds_impl(
                        &code_unit.jump_tables,
                        *jti,
                        bytecode_offset,
                    )?;
                }
            }
        }

        for VariantJumpTable {
            head_enum,
            jump_table,
        } in code_unit.jump_tables.iter()
        {
            check_bounds_impl(self.module.enum_defs(), *head_enum)?;
            let enum_defs = self.module.enum_defs();
            let num_variants = enum_defs[head_enum.into_index()].variants.len();
            let code_len = code_unit.code.len();
            let JumpTableInner::Full(jump_table) = jump_table;
            let jt_len = jump_table.len();
            if jt_len != num_variants {
                return Err(verification_error(
                    StatusCode::INVALID_ENUM_SWITCH,
                    IndexKind::VariantTag,
                    jt_len as TableIndex,
                )
                .with_message(format!(
                    "Jump table length {} does not equal number of variants {}",
                    jt_len, num_variants,
                )));
            }
            for offset in jump_table {
                if *offset as usize >= code_len {
                    return Err(bounds_error(
                        StatusCode::INDEX_OUT_OF_BOUNDS,
                        IndexKind::CodeDefinition,
                        *offset,
                        code_len,
                    ));
                }
            }
        }
        Ok(())
    }

    fn check_type(&self, ty: &SignatureToken) -> PartialVMResult<()> {
        use self::SignatureToken::*;

        for ty in ty.preorder_traversal() {
            match ty {
                Bool | U8 | U16 | U32 | U64 | U128 | U256 | Address | Signer | TypeParameter(_)
                | Reference(_) | MutableReference(_) | Vector(_) => (),
                Datatype(idx) => {
                    check_bounds_impl(self.module.datatype_handles(), *idx)?;
                    if let Some(sh) = self.module.datatype_handles().get(idx.into_index()) {
                        if !sh.type_parameters.is_empty() {
                            return Err(PartialVMError::new(
                                StatusCode::NUMBER_OF_TYPE_ARGUMENTS_MISMATCH,
                            )
                            .with_message(format!(
                                "expected {} type parameters got 0 (Struct)",
                                sh.type_parameters.len(),
                            )));
                        }
                    }
                }
                DatatypeInstantiation(inst) => {
                    let (idx, type_params) = &**inst;
                    check_bounds_impl(self.module.datatype_handles(), *idx)?;
                    if let Some(sh) = self.module.datatype_handles().get(idx.into_index()) {
                        if sh.type_parameters.len() != type_params.len() {
                            return Err(PartialVMError::new(
                                StatusCode::NUMBER_OF_TYPE_ARGUMENTS_MISMATCH,
                            )
                            .with_message(format!(
                                "expected {} type parameters got {}",
                                sh.type_parameters.len(),
                                type_params.len(),
                            )));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn check_type_parameter(
        &self,
        ty: &SignatureToken,
        type_param_count: usize,
    ) -> PartialVMResult<()> {
        use self::SignatureToken::*;

        for ty in ty.preorder_traversal() {
            match ty {
                SignatureToken::TypeParameter(idx) => {
                    if *idx as usize >= type_param_count {
                        return Err(bounds_error(
                            StatusCode::INDEX_OUT_OF_BOUNDS,
                            IndexKind::TypeParameter,
                            *idx,
                            type_param_count,
                        ));
                    }
                }

                Bool
                | U8
                | U16
                | U32
                | U64
                | U128
                | U256
                | Address
                | Signer
                | Datatype(_)
                | Reference(_)
                | MutableReference(_)
                | Vector(_)
                | DatatypeInstantiation(_) => (),
            }
        }
        Ok(())
    }

    fn check_code_unit_bounds_impl<T, I>(
        &self,
        pool: &[T],
        idx: I,
        bytecode_offset: usize,
    ) -> PartialVMResult<()>
    where
        I: ModuleIndex,
    {
        let idx = idx.into_index();
        let len = pool.len();
        if idx >= len {
            Err(self.offset_out_of_bounds(
                StatusCode::INDEX_OUT_OF_BOUNDS,
                I::KIND,
                idx,
                len,
                bytecode_offset as CodeOffset,
            ))
        } else {
            Ok(())
        }
    }

    fn get_locals(&self, code_unit: &CodeUnit) -> PartialVMResult<&[SignatureToken]> {
        match self.module.signatures().get(code_unit.locals.into_index()) {
            Some(signature) => Ok(&signature.0),
            None => Err(bounds_error(
                StatusCode::INDEX_OUT_OF_BOUNDS,
                IndexKind::Signature,
                code_unit.locals.into_index() as u16,
                self.module.signatures().len(),
            )),
        }
    }

    fn offset_out_of_bounds(
        &self,
        status: StatusCode,
        kind: IndexKind,
        target_offset: usize,
        target_pool_len: usize,
        cur_bytecode_offset: CodeOffset,
    ) -> PartialVMError {
        match self.context {
            BoundsCheckingContext::Module => {
                let msg = format!("Indexing into bytecode {} during bounds checking but 'current_function' was not set", cur_bytecode_offset);
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR).with_message(msg)
            }
            BoundsCheckingContext::ModuleFunction(current_function_index) => {
                offset_out_of_bounds_error(
                    status,
                    kind,
                    target_offset,
                    target_pool_len,
                    current_function_index,
                    cur_bytecode_offset,
                )
            }
        }
    }
}

fn check_bounds_impl<T, I>(pool: &[T], idx: I) -> PartialVMResult<()>
where
    I: ModuleIndex,
{
    let idx = idx.into_index();
    let len = pool.len();
    if idx >= len {
        Err(bounds_error(
            StatusCode::INDEX_OUT_OF_BOUNDS,
            I::KIND,
            idx as TableIndex,
            len,
        ))
    } else {
        Ok(())
    }
}
