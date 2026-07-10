// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{
    IndexKind,
    errors::{Location, PartialVMError, PartialVMResult, VMResult, verification_error},
    file_format::{Bytecode, CompiledModule, SignatureToken, StructFieldInformation, TableIndex},
};
use move_core_types::{runtime_value::MoveValue, vm_status::StatusCode};
use move_vm_config::verifier::VerifierConfig;
use std::collections::BTreeMap;

pub struct LimitsVerifier<'a> {
    module: &'a CompiledModule,
}

const STRUCT_SIZE_WEIGHT: usize = 4;
const PARAM_SIZE_WEIGHT: usize = 4;

fn weighted_type_size(ty: &SignatureToken) -> usize {
    let mut size = 0usize;
    for t in ty.preorder_traversal() {
        let inc = match t {
            SignatureToken::Datatype(..) | SignatureToken::DatatypeInstantiation(..) => {
                STRUCT_SIZE_WEIGHT
            }
            SignatureToken::TypeParameter(..) => PARAM_SIZE_WEIGHT,
            _ => 1,
        };
        size = size.saturating_add(inc);
    }
    size
}

impl<'a> LimitsVerifier<'a> {
    pub fn verify_module(config: &VerifierConfig, module: &'a CompiledModule) -> VMResult<()> {
        Self::verify_module_impl(config, module)
            .map_err(|e| e.finish(Location::Module(module.self_id())))
    }

    fn verify_module_impl(
        config: &VerifierConfig,
        module: &'a CompiledModule,
    ) -> PartialVMResult<()> {
        let limit_check = Self { module };
        limit_check.verify_constants(config)?;
        limit_check.verify_function_handles(config)?;
        limit_check.verify_datatype_handles(config)?;
        limit_check.verify_type_nodes(config)?;
        limit_check.verify_identifiers(config)?;
        limit_check.verify_definitions(config)?;
        limit_check.verify_generic_instantiations(config)
    }
    fn verify_datatype_handles(&self, config: &VerifierConfig) -> PartialVMResult<()> {
        if let Some(limit) = config.max_generic_instantiation_length {
            for (idx, struct_handle) in self.module.datatype_handles().iter().enumerate() {
                if struct_handle.type_parameters.len() > limit {
                    return Err(PartialVMError::new(StatusCode::TOO_MANY_TYPE_PARAMETERS)
                        .at_index(IndexKind::DatatypeHandle, idx as u16));
                }
            }
        }
        Ok(())
    }

    fn verify_function_handles(&self, config: &VerifierConfig) -> PartialVMResult<()> {
        for (idx, function_handle) in self.module.function_handles().iter().enumerate() {
            if let Some(limit) = config.max_generic_instantiation_length
                && function_handle.type_parameters.len() > limit
            {
                return Err(PartialVMError::new(StatusCode::TOO_MANY_TYPE_PARAMETERS)
                    .at_index(IndexKind::FunctionHandle, idx as u16));
            };
            if let Some(limit) = config.max_function_parameters
                && self.module.signature_at(function_handle.parameters).0.len() > limit
            {
                return Err(PartialVMError::new(StatusCode::TOO_MANY_PARAMETERS)
                    .at_index(IndexKind::FunctionHandle, idx as u16));
            };
        }
        Ok(())
    }

    fn verify_type_nodes(&self, config: &VerifierConfig) -> PartialVMResult<()> {
        for sign in self.module.signatures() {
            for ty in &sign.0 {
                self.verify_type_node(config, ty)?
            }
        }
        for cons in self.module.constant_pool() {
            self.verify_type_node(config, &cons.type_)?
        }

        for sdef in self.module.struct_defs() {
            if let StructFieldInformation::Declared(fdefs) = &sdef.field_information {
                for fdef in fdefs {
                    self.verify_type_node(config, &fdef.signature.0)?
                }
            }
        }

        for field in self
            .module
            .enum_defs()
            .iter()
            .flat_map(|e| e.variants.iter().flat_map(|v| &v.fields))
        {
            self.verify_type_node(config, &field.signature.0)?
        }
        Ok(())
    }

    fn verify_type_node(
        &self,
        config: &VerifierConfig,
        ty: &SignatureToken,
    ) -> PartialVMResult<()> {
        if let Some(max) = &config.max_type_nodes
            && weighted_type_size(ty) > *max
        {
            return Err(PartialVMError::new(StatusCode::TOO_MANY_TYPE_NODES));
        }
        Ok(())
    }

    fn verify_definitions(&self, config: &VerifierConfig) -> PartialVMResult<()> {
        let defs = self.module.function_defs();
        if let Some(max_function_definitions) = config.max_function_definitions
            && defs.len() > max_function_definitions
        {
            return Err(PartialVMError::new(
                StatusCode::MAX_FUNCTION_DEFINITIONS_REACHED,
            ));
        }
        if let Some(max_data_definitions) = config.max_data_definitions {
            let defs_len = self.module.struct_defs().len() + self.module.enum_defs().len();
            if defs_len > max_data_definitions {
                return Err(PartialVMError::new(
                    StatusCode::MAX_STRUCT_DEFINITIONS_REACHED,
                ));
            }
        }

        if let Some(max_fields_in_struct) = config.max_fields_in_struct {
            for def in self.module.struct_defs() {
                match &def.field_information {
                    StructFieldInformation::Native => (),
                    StructFieldInformation::Declared(fields) => {
                        if fields.len() > max_fields_in_struct {
                            return Err(PartialVMError::new(
                                StatusCode::MAX_FIELD_DEFINITIONS_REACHED,
                            ));
                        }
                    }
                }
            }

            // 1. Total number of fields in the enum (added across all variants) is less than
            //    the number of fields allowed in a struct.
            // 2. Total number of variants in the enum is less than the number of variants allowed in an enum.
            for def in self.module.enum_defs() {
                if config
                    .max_variants_in_enum
                    .is_some_and(|max| def.variants.len() > max as usize)
                {
                    return Err(PartialVMError::new(StatusCode::MAX_VARIANTS_REACHED));
                }
                let mut num_fields = 0;
                for variant in &def.variants {
                    num_fields += variant.fields.len();
                    if num_fields > max_fields_in_struct {
                        return Err(PartialVMError::new(
                            StatusCode::MAX_FIELD_DEFINITIONS_REACHED,
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    fn verify_constants(&self, config: &VerifierConfig) -> PartialVMResult<()> {
        for (idx, constant) in self.module.constant_pool().iter().enumerate() {
            if let SignatureToken::Vector(_) = constant.type_ {
                if let MoveValue::Vector(cons) =
                    constant.deserialize_constant().ok_or_else(|| {
                        verification_error(
                            StatusCode::MALFORMED_CONSTANT_DATA,
                            IndexKind::ConstantPool,
                            idx as TableIndex,
                        )
                    })?
                {
                    if let Some(lim) = config.max_constant_vector_len
                        && cons.len() > lim as usize
                    {
                        return Err(PartialVMError::new(StatusCode::TOO_MANY_VECTOR_ELEMENTS)
                            .with_message(format!("vector size limit is {}", lim)));
                    }
                } else {
                    return Err(verification_error(
                        StatusCode::INVALID_CONSTANT_TYPE,
                        IndexKind::ConstantPool,
                        idx as TableIndex,
                    ));
                }
            }
        }
        Ok(())
    }

    fn verify_generic_instantiations(&self, config: &VerifierConfig) -> PartialVMResult<()> {
        let max_fun = config.max_generic_instantiation_type_nodes_per_function;
        let max_mod = config.max_generic_instantiation_type_nodes_per_module;
        if max_fun.is_none() && max_mod.is_none() {
            return Ok(());
        }

        let mut module_total: usize = 0;
        let mut size_table = BTreeMap::new();
        for func_def in self.module.function_defs() {
            let Some(code) = &func_def.code else { continue };
            let mut fn_total: usize = 0;
            for instr in &code.code {
                let sig_idx = match instr {
                    Bytecode::CallGeneric(idx) => {
                        self.module.function_instantiation_at(*idx).type_parameters
                    }
                    Bytecode::PackGeneric(idx)
                    | Bytecode::UnpackGeneric(idx)
                    | Bytecode::ExistsGenericDeprecated(idx)
                    | Bytecode::MoveFromGenericDeprecated(idx)
                    | Bytecode::MoveToGenericDeprecated(idx)
                    | Bytecode::ImmBorrowGlobalGenericDeprecated(idx)
                    | Bytecode::MutBorrowGlobalGenericDeprecated(idx) => {
                        self.module.struct_instantiation_at(*idx).type_parameters
                    }
                    Bytecode::ImmBorrowFieldGeneric(idx) | Bytecode::MutBorrowFieldGeneric(idx) => {
                        self.module.field_instantiation_at(*idx).type_parameters
                    }
                    Bytecode::VecPack(idx, _)
                    | Bytecode::VecLen(idx)
                    | Bytecode::VecImmBorrow(idx)
                    | Bytecode::VecMutBorrow(idx)
                    | Bytecode::VecPushBack(idx)
                    | Bytecode::VecPopBack(idx)
                    | Bytecode::VecUnpack(idx, _)
                    | Bytecode::VecSwap(idx) => *idx,
                    Bytecode::PackVariantGeneric(vidx)
                    | Bytecode::UnpackVariantGeneric(vidx)
                    | Bytecode::UnpackVariantGenericImmRef(vidx)
                    | Bytecode::UnpackVariantGenericMutRef(vidx) => {
                        let handle = self.module.variant_instantiation_handle_at(*vidx);
                        self.module
                            .enum_instantiation_at(handle.enum_def)
                            .type_parameters
                    }
                    // List out the other options explicitly so there's a compile error if a new
                    // bytecode gets added.
                    Bytecode::Pop
                    | Bytecode::Ret
                    | Bytecode::BrTrue(_)
                    | Bytecode::BrFalse(_)
                    | Bytecode::Branch(_)
                    | Bytecode::LdU8(_)
                    | Bytecode::LdU64(_)
                    | Bytecode::LdU128(_)
                    | Bytecode::CastU8
                    | Bytecode::CastU64
                    | Bytecode::CastU128
                    | Bytecode::LdConst(_)
                    | Bytecode::LdTrue
                    | Bytecode::LdFalse
                    | Bytecode::CopyLoc(_)
                    | Bytecode::MoveLoc(_)
                    | Bytecode::StLoc(_)
                    | Bytecode::Call(_)
                    | Bytecode::Pack(_)
                    | Bytecode::Unpack(_)
                    | Bytecode::ReadRef
                    | Bytecode::WriteRef
                    | Bytecode::FreezeRef
                    | Bytecode::MutBorrowLoc(_)
                    | Bytecode::ImmBorrowLoc(_)
                    | Bytecode::MutBorrowField(_)
                    | Bytecode::ImmBorrowField(_)
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
                    | Bytecode::Not
                    | Bytecode::Eq
                    | Bytecode::Neq
                    | Bytecode::Lt
                    | Bytecode::Gt
                    | Bytecode::Le
                    | Bytecode::Ge
                    | Bytecode::Abort
                    | Bytecode::Nop
                    | Bytecode::Shl
                    | Bytecode::Shr
                    | Bytecode::LdU16(_)
                    | Bytecode::LdU32(_)
                    | Bytecode::LdU256(_)
                    | Bytecode::CastU16
                    | Bytecode::CastU32
                    | Bytecode::CastU256
                    | Bytecode::PackVariant(_)
                    | Bytecode::UnpackVariant(_)
                    | Bytecode::UnpackVariantImmRef(_)
                    | Bytecode::UnpackVariantMutRef(_)
                    | Bytecode::VariantSwitch(_)
                    | Bytecode::ExistsDeprecated(_)
                    | Bytecode::MoveFromDeprecated(_)
                    | Bytecode::MoveToDeprecated(_)
                    | Bytecode::MutBorrowGlobalDeprecated(_)
                    | Bytecode::ImmBorrowGlobalDeprecated(_) => continue,
                };

                let weight = *size_table.entry(sig_idx).or_insert_with(|| {
                    self.module
                        .signature_at(sig_idx)
                        .0
                        .iter()
                        .fold(0usize, |acc, ty| acc.saturating_add(weighted_type_size(ty)))
                });
                fn_total = fn_total.saturating_add(weight);
                module_total = module_total.saturating_add(weight);

                if let Some(max) = max_fun
                    && fn_total > max
                {
                    return Err(
                        PartialVMError::new(StatusCode::TOO_MANY_TYPE_NODES).with_message(format!(
                            "function exceeds generic-instantiation budget: {} > {}",
                            fn_total, max
                        )),
                    );
                }

                if let Some(max) = max_mod
                    && module_total > max
                {
                    return Err(
                        PartialVMError::new(StatusCode::TOO_MANY_TYPE_NODES).with_message(format!(
                            "module exceeds generic-instantiation budget: {} > {}",
                            module_total, max
                        )),
                    );
                }
            }
        }
        Ok(())
    }

    /// Verifies the lengths of all identifers are valid
    fn verify_identifiers(&self, config: &VerifierConfig) -> PartialVMResult<()> {
        for (idx, identifier) in self.module.identifiers().iter().enumerate() {
            if config
                .max_identifier_len
                .is_some_and(|max_identifier_len| identifier.len() > (max_identifier_len as usize))
            {
                return Err(verification_error(
                    StatusCode::IDENTIFIER_TOO_LONG,
                    IndexKind::Identifier,
                    idx as TableIndex,
                ));
            }

            if config.disallow_self_identifier && identifier.as_str() == "<SELF>" {
                return Err(verification_error(
                    StatusCode::INVALID_IDENTIFIER,
                    IndexKind::Identifier,
                    idx as TableIndex,
                ));
            }
        }

        Ok(())
    }
}
