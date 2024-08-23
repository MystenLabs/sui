// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{
    errors::{verification_error, Location, PartialVMError, PartialVMResult, VMResult},
    file_format::{CompiledModule, SignatureToken, StructFieldInformation, TableIndex},
    IndexKind,
};
use move_core_types::{runtime_value::MoveValue, vm_status::StatusCode};
use move_vm_config::verifier::VerifierConfig;

pub struct LimitsVerifier<'a> {
    module: &'a CompiledModule,
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
        limit_check.verify_definitions(config)
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
            if let Some(limit) = config.max_generic_instantiation_length {
                if function_handle.type_parameters.len() > limit {
                    return Err(PartialVMError::new(StatusCode::TOO_MANY_TYPE_PARAMETERS)
                        .at_index(IndexKind::FunctionHandle, idx as u16));
                }
            };
            if let Some(limit) = config.max_function_parameters {
                if self.module.signature_at(function_handle.parameters).0.len() > limit {
                    return Err(PartialVMError::new(StatusCode::TOO_MANY_PARAMETERS)
                        .at_index(IndexKind::FunctionHandle, idx as u16));
                }
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
        if let Some(max) = &config.max_type_nodes {
            // Structs and Parameters can expand to an unknown number of nodes, therefore
            // we give them a higher size weight here.
            const STRUCT_SIZE_WEIGHT: usize = 4;
            const PARAM_SIZE_WEIGHT: usize = 4;
            let mut size = 0;
            for t in ty.preorder_traversal() {
                // Notice that the preorder traversal will iterate all type instantiations, so we
                // why we can ignore them below.
                match t {
                    SignatureToken::Datatype(..) | SignatureToken::DatatypeInstantiation(..) => {
                        size += STRUCT_SIZE_WEIGHT
                    }
                    SignatureToken::TypeParameter(..) => size += PARAM_SIZE_WEIGHT,
                    _ => size += 1,
                }
            }
            if size > *max {
                return Err(PartialVMError::new(StatusCode::TOO_MANY_TYPE_NODES));
            }
        }
        Ok(())
    }

    fn verify_definitions(&self, config: &VerifierConfig) -> PartialVMResult<()> {
        let defs = self.module.function_defs();
        if let Some(max_function_definitions) = config.max_function_definitions {
            if defs.len() > max_function_definitions {
                return Err(PartialVMError::new(
                    StatusCode::MAX_FUNCTION_DEFINITIONS_REACHED,
                ));
            }
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
                    if let Some(lim) = config.max_constant_vector_len {
                        if cons.len() > lim as usize {
                            return Err(PartialVMError::new(StatusCode::TOO_MANY_VECTOR_ELEMENTS)
                                .with_message(format!("vector size limit is {}", lim)));
                        }
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

    /// Verifies the lengths of all identifers are valid
    fn verify_identifiers(&self, config: &VerifierConfig) -> PartialVMResult<()> {
        if let Some(max_idenfitier_len) = config.max_idenfitier_len {
            for (idx, identifier) in self.module.identifiers().iter().enumerate() {
                if identifier.len() > (max_idenfitier_len as usize) {
                    return Err(verification_error(
                        StatusCode::IDENTIFIER_TOO_LONG,
                        IndexKind::Identifier,
                        idx as TableIndex,
                    ));
                }
            }
        }
        Ok(())
    }
}
