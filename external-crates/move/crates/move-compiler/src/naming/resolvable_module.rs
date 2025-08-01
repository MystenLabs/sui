// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    expansion::ast as E,
    naming::{ast as N, translate::FieldInfo},
    parser::ast::{ConstantName, DatatypeName, FunctionName, VariantName},
    shared::{Identifier, program_info::ModuleInfo},
};
use move_ir_types::location::*;

/// Trait to abstract over module-like structures for member resolution
pub(crate) trait ResolvableModule {
    /// Returns iterator over structs: (name, type_params_len, field_info, loc)
    fn structs(&self) -> Box<dyn Iterator<Item = (DatatypeName, usize, FieldInfo, Loc)> + '_>;

    /// Returns iterator over enums: (name, type_params_len, loc, variants)
    /// where variants is Vec of (variant_name, field_info, variant_loc)
    fn enums(
        &self,
    ) -> Box<dyn Iterator<Item = (DatatypeName, usize, Loc, Vec<(VariantName, FieldInfo, Loc)>)> + '_>;

    /// Returns iterator over functions: (name, type_params_len, params_len)
    fn functions(&self) -> Box<dyn Iterator<Item = (FunctionName, usize, usize)> + '_>;

    /// Returns iterator over constants: (name, defined_loc)
    fn constants(&self) -> Box<dyn Iterator<Item = (ConstantName, Loc)> + '_>;
}

impl ResolvableModule for E::ModuleDefinition {
    fn structs(&self) -> Box<dyn Iterator<Item = (DatatypeName, usize, FieldInfo, Loc)> + '_> {
        Box::new(self.structs.key_cloned_iter().map(|(name, sdef)| {
            let field_info = match &sdef.fields {
                E::StructFields::Positional(fields) => FieldInfo::Positional(fields.len()),
                E::StructFields::Named(f) => {
                    FieldInfo::Named(f.key_cloned_iter().map(|(k, _)| k).collect())
                }
                E::StructFields::Native(_) => FieldInfo::Empty,
            };
            (name, sdef.type_parameters.len(), field_info, name.loc())
        }))
    }

    fn enums(
        &self,
    ) -> Box<dyn Iterator<Item = (DatatypeName, usize, Loc, Vec<(VariantName, FieldInfo, Loc)>)> + '_>
    {
        Box::new(self.enums.key_cloned_iter().map(|(enum_name, edef)| {
            let variants: Vec<_> = edef
                .variants
                .key_cloned_iter()
                .map(|(vname, vdef)| {
                    let field_info = match &vdef.fields {
                        E::VariantFields::Named(fields) => {
                            FieldInfo::Named(fields.key_cloned_iter().map(|(k, _)| k).collect())
                        }
                        E::VariantFields::Positional(tys) => FieldInfo::Positional(tys.len()),
                        E::VariantFields::Empty => FieldInfo::Empty,
                    };
                    (vname, field_info, vdef.loc)
                })
                .collect();
            (enum_name, edef.type_parameters.len(), edef.loc, variants)
        }))
    }

    fn functions(&self) -> Box<dyn Iterator<Item = (FunctionName, usize, usize)> + '_> {
        Box::new(self.functions.key_cloned_iter().map(|(name, fun)| {
            (
                name,
                fun.signature.type_parameters.len(),
                fun.signature.parameters.len(),
            )
        }))
    }

    fn constants(&self) -> Box<dyn Iterator<Item = (ConstantName, Loc)> + '_> {
        Box::new(
            self.constants
                .key_cloned_iter()
                .map(|(name, _)| (name, name.loc())),
        )
    }
}

impl ResolvableModule for ModuleInfo {
    fn structs(&self) -> Box<dyn Iterator<Item = (DatatypeName, usize, FieldInfo, Loc)> + '_> {
        Box::new(self.structs.key_cloned_iter().map(|(name, sdef)| {
            let field_info = match &sdef.fields {
                N::StructFields::Defined(positional, fields) => {
                    if *positional {
                        FieldInfo::Positional(fields.len())
                    } else {
                        FieldInfo::Named(fields.key_cloned_iter().map(|(k, _)| k).collect())
                    }
                }
                N::StructFields::Native(_) => FieldInfo::Empty,
            };
            (name, sdef.type_parameters.len(), field_info, name.loc())
        }))
    }

    fn enums(
        &self,
    ) -> Box<dyn Iterator<Item = (DatatypeName, usize, Loc, Vec<(VariantName, FieldInfo, Loc)>)> + '_>
    {
        Box::new(self.enums.key_cloned_iter().map(|(enum_name, edef)| {
            let variants: Vec<_> = edef
                .variants
                .key_cloned_iter()
                .map(|(vname, vdef)| {
                    let field_info = match &vdef.fields {
                        N::VariantFields::Defined(positional, fields) => {
                            if *positional {
                                FieldInfo::Positional(fields.len())
                            } else {
                                FieldInfo::Named(fields.key_cloned_iter().map(|(k, _)| k).collect())
                            }
                        }
                        N::VariantFields::Empty => FieldInfo::Empty,
                    };
                    (vname, field_info, vdef.loc)
                })
                .collect();
            (enum_name, edef.type_parameters.len(), edef.loc, variants)
        }))
    }

    fn functions(&self) -> Box<dyn Iterator<Item = (FunctionName, usize, usize)> + '_> {
        Box::new(self.functions.key_cloned_iter().map(|(name, finfo)| {
            (
                name,
                finfo.signature.type_parameters.len(),
                finfo.signature.parameters.len(),
            )
        }))
    }

    fn constants(&self) -> Box<dyn Iterator<Item = (ConstantName, Loc)> + '_> {
        Box::new(
            self.constants
                .key_cloned_iter()
                .map(|(name, cinfo)| (name, cinfo.defined_loc)),
        )
    }
}
