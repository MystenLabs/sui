// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::module_cache::GetModule;
use anyhow::{bail, Result};
use move_binary_format::{
    access::ModuleAccess,
    file_format::{
        DatatypeHandleIndex, DatatypeTyParameter, EnumDefinition, SignatureToken, StructDefinition,
        StructFieldInformation,
    },
    normalized::{Enum, Struct, Type},
    CompiledModule,
};
use move_core_types::{
    account_address::AccountAddress,
    annotated_value as A,
    identifier::{IdentStr, Identifier},
    language_storage::{ModuleId, StructTag, TypeTag},
};
use serde_reflection::{ContainerFormat, Format, Named, Registry, VariantFormat};
use std::{borrow::Borrow, collections::BTreeMap, fmt::Write};

/// Name of the Move `address` type in the serde registry
const ADDRESS: &str = "AccountAddress";

/// Name of the Move `signer` type in the serde registry
const SIGNER: &str = "Signer";

/// Name of the Move `u256` type in the serde registry
const U256_SERDE_NAME: &str = "u256";

/// The maximal value depth that we allow creating a layout for.
const MAX_VALUE_DEPTH: u64 = 128;

macro_rules! check_depth {
    ($depth:expr) => {
        if $depth > MAX_VALUE_DEPTH {
            anyhow::bail!("Exceeded max value recursion depth when creating struct layout")
        }
    };
}

enum Container {
    Struct(Struct),
    Enum(Enum),
}

impl Container {
    fn type_parameters(&self) -> &[DatatypeTyParameter] {
        match self {
            Container::Struct(s) => &s.type_parameters,
            Container::Enum(e) => &e.type_parameters,
        }
    }
}

/// Type for building a registry of serde-reflection friendly struct layouts for Move types.
/// The layouts created by this type are intended to be passed to the serde-generate tool to create
/// struct bindings for Move types in source languages that use Move-based services.
pub struct SerdeLayoutBuilder<'a, T> {
    registry: Registry,
    module_resolver: &'a T,
    config: SerdeLayoutConfig,
}

#[derive(Default)]
pub struct SerdeLayoutConfig {
    /// If separator is Some, replace all Move source syntax separators ("::" for address/struct/module name
    /// separation, "<", ">", and "," for generics separation) with this string.
    /// If separator is None, use the same syntax as Move source
    pub separator: Option<String>,
    /// If true, do not include addresses in fully qualified type names.
    /// If there is a name conflict (e.g., the registry we're building has both
    /// 0x1::M::T and 0x2::M::T), layout generation will fail when this option is true.
    pub omit_addresses: bool,
    /// If true, do not include phantom types in fully qualified type names, since they do not contribute to the layout
    /// E.g., if we have `struct S<phantom T> { u: 64 }` and try to generate bindings for this struct with `T = u8`,
    /// the name for `S` in the registry will be `S<u64>` when this option is false, and `S` when this option is true
    pub ignore_phantom_types: bool,
    /// The LayoutBuilder can operate in two modes: "deep" and "shallow".
    /// In shallow mode, generate a single layout for the struct or type passed in by the user
    /// (under the assumption that layouts for dependencies have been generated previously).
    /// In deep mode, it generate layouts for all of the (transitive) dependencies of the type passed
    /// in, as well as layouts for the Move ground types like `address` and `signer`. The result is a
    /// self-contained registry with no unbound typenames
    pub shallow: bool,
}

impl<'a, T: GetModule> SerdeLayoutBuilder<'a, T> {
    /// Create a `LayoutBuilder` with an empty registry and deep layout resolution
    pub fn new(module_resolver: &'a T) -> Self {
        Self {
            registry: Self::default_registry(),
            module_resolver,
            config: SerdeLayoutConfig::default(),
        }
    }

    /// Create a `LayoutBuilder` with an empty registry and shallow layout resolution
    pub fn new_with_config(module_resolver: &'a T, config: SerdeLayoutConfig) -> Self {
        Self {
            registry: Self::default_registry(),
            module_resolver,
            config,
        }
    }

    /// Return a registry containing layouts for all the Move ground types (e.g., address)
    pub fn default_registry() -> Registry {
        let mut registry = BTreeMap::new();
        // add Move ground types to registry (address, signer)
        let address_layout = Box::new(Format::TupleArray {
            content: Box::new(Format::U8),
            size: AccountAddress::LENGTH,
        });
        registry.insert(
            ADDRESS.to_string(),
            ContainerFormat::NewTypeStruct(address_layout.clone()),
        );
        registry.insert(
            SIGNER.to_string(),
            ContainerFormat::NewTypeStruct(address_layout),
        );

        registry
    }

    /// Get the registry of layouts generated so far
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Get the registry of layouts generated so far
    pub fn into_registry(self) -> Registry {
        self.registry
    }

    /// Add layouts for all types used in `t` to the registry
    pub fn build_type_layout(&mut self, t: TypeTag) -> Result<Format> {
        self.build_normalized_type_layout(&Type::from(t), &Vec::new(), 0)
    }

    /// Add layouts for all types used in `t` to the registry
    pub fn build_data_layout(&mut self, s: &StructTag) -> Result<Format> {
        let serde_type_args = s
            .type_params
            .iter()
            .map(|t| self.build_type_layout(t.clone()))
            .collect::<Result<Vec<Format>>>()?;
        self.build_data_layout_(&s.module_id(), &s.name, &serde_type_args, 0)
    }

    fn build_normalized_type_layout(
        &mut self,
        t: &Type,
        input_type_args: &[Format],
        depth: u64,
    ) -> Result<Format> {
        use Type::*;
        check_depth!(depth);
        Ok(match t {
            Bool => Format::Bool,
            U8 => Format::U8,
            U16 => Format::U16,
            U32 => Format::U32,
            U64 => Format::U64,
            U128 => Format::U128,
            U256 => Format::TypeName(U256_SERDE_NAME.to_string()),
            Address => Format::TypeName(ADDRESS.to_string()),
            Signer => Format::TypeName(SIGNER.to_string()),
            Struct {
                address,
                module,
                name,
                type_arguments,
            } => {
                let serde_type_args = type_arguments
                    .iter()
                    .map(|t| self.build_normalized_type_layout(t, input_type_args, depth + 1))
                    .collect::<Result<Vec<Format>>>()?;
                let declaring_module = ModuleId::new(*address, module.clone());
                self.build_data_layout_(&declaring_module, name, &serde_type_args, depth + 1)?
            }
            Vector(inner_t) => {
                if matches!(inner_t.as_ref(), U8) {
                    // specialize vector<u8> as bytes
                    Format::Bytes
                } else {
                    Format::Seq(Box::new(self.build_normalized_type_layout(
                        inner_t,
                        input_type_args,
                        depth + 1,
                    )?))
                }
            }
            TypeParameter(i) => input_type_args[*i as usize].clone(),
            Reference(_) | MutableReference(_) => unreachable!(), // structs cannot store references
        })
    }

    fn build_data_layout_(
        &mut self,
        module_id: &ModuleId,
        name: &Identifier,
        type_arguments: &[Format],
        depth: u64,
    ) -> Result<Format> {
        check_depth!(depth);

        // build a human-readable name for the struct type. this should do the same thing as
        // StructTag::display(), but it's not easy to use that code here

        let declaring_module = self
            .module_resolver
            .get_module_by_id(module_id)
            .map_err(|e| anyhow::format_err!("{:?}", e))?
            .expect("Failed to resolve module");
        let def = match (
            declaring_module.borrow().find_struct_def_by_name(name),
            declaring_module.borrow().find_enum_def_by_name(name),
        ) {
            (Some(struct_def), None) => {
                Container::Struct(Struct::new(declaring_module.borrow(), struct_def).1)
            }
            (None, Some(enum_def)) => {
                Container::Enum(Enum::new(declaring_module.borrow(), enum_def).1)
            }
            (Some(_), Some(_)) => panic!("Found both struct and enum with name {}", name),
            (None, None) => panic!(
                "Could not find struct named {} in module {}",
                name,
                declaring_module.borrow().name()
            ),
        };
        assert_eq!(
            def.type_parameters().len(),
            type_arguments.len(),
            "Wrong number of type arguments for struct"
        );

        let generics: Vec<String> = type_arguments
            .iter()
            .zip(def.type_parameters().iter())
            .filter(|(_, type_param)| {
                // do not include phantom type arguments in the struct key, since they do not affect the struct layout
                !(self.config.ignore_phantom_types && type_param.is_phantom)
            })
            .map(|(type_arg, _)| print_format_type(type_arg, depth))
            .collect::<Result<_>>()?;
        let mut type_key = String::new();
        if !self.config.omit_addresses {
            write!(
                type_key,
                "{}{}",
                module_id.address(),
                self.config.separator.as_deref().unwrap_or("::")
            )
            .unwrap();
        }
        write!(
            type_key,
            "{}{}{}",
            module_id.name(),
            self.config.separator.as_deref().unwrap_or("::"),
            name
        )
        .unwrap();
        if !generics.is_empty() {
            write!(
                type_key,
                "{}{}{}",
                self.config.separator.as_deref().unwrap_or("<"),
                generics.join(self.config.separator.as_deref().unwrap_or(",")),
                self.config.separator.as_deref().unwrap_or(">")
            )
            .unwrap()
        }
        if self.config.shallow {
            return Ok(Format::TypeName(type_key));
        }

        if let Some(old_datatype) = self.registry.get(&type_key) {
            if self.config.omit_addresses || self.config.separator.is_some() {
                // check for conflicts (e.g., 0x1::M::T and 0x2::M::T that both get stripped to M::T because
                // omit_addresses is on)
                if old_datatype.clone()
                    != self.generate_serde_container(def, type_arguments, depth)?
                {
                    panic!(
                        "Name conflict: multiple structs with name {}, but different addresses",
                        type_key
                    )
                }
            }
        } else {
            // not found--generate and update registry
            let serde_data = self.generate_serde_container(def, type_arguments, depth)?;
            self.registry.insert(type_key.clone(), serde_data);
        }

        Ok(Format::TypeName(type_key))
    }

    fn generate_serde_container(
        &mut self,
        container: Container,
        type_arguments: &[Format],
        depth: u64,
    ) -> Result<ContainerFormat> {
        match container {
            Container::Struct(s) => self.generate_serde_struct(s, type_arguments, depth),
            Container::Enum(e) => self.generate_serde_enum(e, type_arguments, depth),
        }
    }

    fn generate_serde_struct(
        &mut self,
        normalized_struct: Struct,
        type_arguments: &[Format],
        depth: u64,
    ) -> Result<ContainerFormat> {
        check_depth!(depth);
        let fields = normalized_struct
            .fields
            .iter()
            .map(|f| {
                self.build_normalized_type_layout(&f.type_, type_arguments, depth)
                    .map(|value| Named {
                        name: f.name.to_string(),
                        value,
                    })
            })
            .collect::<Result<Vec<Named<Format>>>>()?;
        Ok(ContainerFormat::Struct(fields))
    }

    fn generate_serde_enum(
        &mut self,
        normalized_enum: Enum,
        type_arguments: &[Format],
        depth: u64,
    ) -> Result<ContainerFormat> {
        check_depth!(depth);
        let variants = normalized_enum
            .variants
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let fields = v
                    .fields
                    .iter()
                    .map(|f| {
                        self.build_normalized_type_layout(&f.type_, type_arguments, depth)
                            .map(|value| Named {
                                name: f.name.to_string(),
                                value,
                            })
                    })
                    .collect::<Result<Vec<Named<Format>>>>();
                fields.map(|fields| {
                    if fields.is_empty() {
                        (
                            i as u32,
                            Named {
                                name: v.name.to_string(),
                                value: VariantFormat::Unit,
                            },
                        )
                    } else {
                        (
                            i as u32,
                            Named {
                                name: v.name.to_string(),
                                value: VariantFormat::Struct(fields),
                            },
                        )
                    }
                })
            })
            .collect::<Result<BTreeMap<u32, Named<VariantFormat>>>>()?;
        Ok(ContainerFormat::Enum(variants))
    }
}

fn print_format_type(t: &Format, depth: u64) -> Result<String> {
    check_depth!(depth);
    Ok(match t {
        Format::TypeName(s) => s.to_string(),
        Format::Bool => "bool".to_string(),
        Format::U8 => "u8".to_string(),
        Format::U16 => "u16".to_string(),
        Format::U32 => "u32".to_string(),
        Format::U64 => "u64".to_string(),
        Format::U128 => "u128".to_string(),
        Format::Bytes => "vector<u8>".to_string(),
        Format::Seq(inner) => format!("vector<{}>", print_format_type(inner, depth + 1)?),
        v => unimplemented!("Printing format value {:?}", v),
    })
}

pub enum TypeLayoutBuilder {}
pub enum DatatypeLayoutBuilder {}

impl TypeLayoutBuilder {
    /// Construct a WithTypes `TypeLayout` with fields from `t`.
    /// Panics if `resolver` cannot resolve a module whose types are referenced directly or
    /// transitively by `t`
    pub fn build_with_types(t: &TypeTag, resolver: &impl GetModule) -> Result<A::MoveTypeLayout> {
        Self::build(t, resolver, 0)
    }

    fn build(t: &TypeTag, resolver: &impl GetModule, depth: u64) -> Result<A::MoveTypeLayout> {
        use TypeTag::*;
        check_depth!(depth);
        Ok(match t {
            Bool => A::MoveTypeLayout::Bool,
            U8 => A::MoveTypeLayout::U8,
            U16 => A::MoveTypeLayout::U16,
            U32 => A::MoveTypeLayout::U32,
            U64 => A::MoveTypeLayout::U64,
            U128 => A::MoveTypeLayout::U128,
            U256 => A::MoveTypeLayout::U256,
            Address => A::MoveTypeLayout::Address,
            Signer => bail!("Type layouts cannot contain signer"),
            Vector(elem_t) => {
                A::MoveTypeLayout::Vector(Box::new(Self::build(elem_t, resolver, depth + 1)?))
            }
            Struct(s) => DatatypeLayoutBuilder::build(s, resolver, depth + 1)?.into_layout(),
        })
    }

    fn build_from_signature_token(
        m: &CompiledModule,
        s: &SignatureToken,
        type_arguments: &[A::MoveTypeLayout],
        resolver: &impl GetModule,
        depth: u64,
    ) -> Result<A::MoveTypeLayout> {
        use SignatureToken::*;
        check_depth!(depth);
        Ok(match s {
            Vector(t) => A::MoveTypeLayout::Vector(Box::new(Self::build_from_signature_token(
                m,
                t,
                type_arguments,
                resolver,
                depth + 1,
            )?)),
            Datatype(shi) => {
                DatatypeLayoutBuilder::build_from_handle_idx(m, *shi, vec![], resolver, depth + 1)?
                    .into_layout()
            }
            DatatypeInstantiation(shi, type_actuals) => {
                let actual_layouts = type_actuals
                    .iter()
                    .map(|t| {
                        Self::build_from_signature_token(m, t, type_arguments, resolver, depth + 1)
                    })
                    .collect::<Result<Vec<_>>>()?;
                DatatypeLayoutBuilder::build_from_handle_idx(
                    m,
                    *shi,
                    actual_layouts,
                    resolver,
                    depth + 1,
                )?
                .into_layout()
            }
            TypeParameter(i) => type_arguments[*i as usize].clone(),
            Bool => A::MoveTypeLayout::Bool,
            U8 => A::MoveTypeLayout::U8,
            U16 => A::MoveTypeLayout::U16,
            U32 => A::MoveTypeLayout::U32,
            U64 => A::MoveTypeLayout::U64,
            U128 => A::MoveTypeLayout::U128,
            U256 => A::MoveTypeLayout::U256,
            Address => A::MoveTypeLayout::Address,
            Signer => bail!("Type layouts cannot contain signer"),
            Reference(_) | MutableReference(_) => bail!("Type layouts cannot contain references"),
        })
    }
}

impl DatatypeLayoutBuilder {
    /// Construct an expanded `TypeLayout` from `s`.
    /// Panics if `resolver` cannot resolved a module whose types are referenced directly or
    /// transitively by `s`.
    fn build(
        s: &StructTag,
        resolver: &impl GetModule,
        depth: u64,
    ) -> Result<A::MoveDatatypeLayout> {
        check_depth!(depth);
        let type_arguments = s
            .type_params
            .iter()
            .map(|t| TypeLayoutBuilder::build(t, resolver, depth))
            .collect::<Result<Vec<A::MoveTypeLayout>>>()?;
        Self::build_from_name(&s.module_id(), &s.name, type_arguments, resolver, depth)
    }

    fn build_from_enum_definition(
        m: &CompiledModule,
        e: &EnumDefinition,
        type_arguments: Vec<A::MoveTypeLayout>,
        resolver: &impl GetModule,
        depth: u64,
    ) -> Result<A::MoveEnumLayout> {
        let e_handle = m.datatype_handle_at(e.enum_handle);
        if e_handle.type_parameters.len() != type_arguments.len() {
            bail!("Wrong number of type arguments for enum")
        }

        let mut variants = BTreeMap::new();
        for (i, variant) in e.variants.iter().enumerate() {
            let name = m.identifier_at(variant.variant_name).to_owned();
            let mut fields = vec![];
            for field in &variant.fields {
                let ty = TypeLayoutBuilder::build_from_signature_token(
                    m,
                    &field.signature.0,
                    &type_arguments,
                    resolver,
                    depth,
                )?;
                let name = m.identifier_at(field.name).to_owned();
                fields.push(A::MoveFieldLayout::new(name, ty));
            }
            variants.insert((name, i as u16), fields);
        }

        let mid = m.self_id();
        let type_params: Vec<TypeTag> = type_arguments.iter().map(|t| t.into()).collect();
        let type_ = StructTag {
            address: *mid.address(),
            module: mid.name().to_owned(),
            name: m.identifier_at(e_handle.name).to_owned(),
            type_params,
        };

        Ok(A::MoveEnumLayout { type_, variants })
    }

    fn build_from_struct_definition(
        m: &CompiledModule,
        s: &StructDefinition,
        type_arguments: Vec<A::MoveTypeLayout>,
        resolver: &impl GetModule,
        depth: u64,
    ) -> Result<A::MoveStructLayout> {
        check_depth!(depth);
        let s_handle = m.datatype_handle_at(s.struct_handle);
        if s_handle.type_parameters.len() != type_arguments.len() {
            bail!("Wrong number of type arguments for struct")
        }
        match &s.field_information {
            StructFieldInformation::Native => {
                bail!("Can't extract fields for native struct")
            }
            StructFieldInformation::Declared(fields) => {
                let layouts = fields
                    .iter()
                    .map(|f| {
                        TypeLayoutBuilder::build_from_signature_token(
                            m,
                            &f.signature.0,
                            &type_arguments,
                            resolver,
                            depth,
                        )
                    })
                    .collect::<Result<Vec<A::MoveTypeLayout>>>()?;

                let mid = m.self_id();
                let type_params: Vec<TypeTag> = type_arguments.iter().map(|t| t.into()).collect();
                let type_ = StructTag {
                    address: *mid.address(),
                    module: mid.name().to_owned(),
                    name: m.identifier_at(s_handle.name).to_owned(),
                    type_params,
                };
                let fields = fields
                    .iter()
                    .map(|f| m.identifier_at(f.name).to_owned())
                    .zip(layouts)
                    .map(|(name, layout)| A::MoveFieldLayout::new(name, layout))
                    .collect();
                Ok(A::MoveStructLayout { type_, fields })
            }
        }
    }

    fn build_from_name(
        declaring_module: &ModuleId,
        name: &IdentStr,
        type_arguments: Vec<A::MoveTypeLayout>,
        resolver: &impl GetModule,
        depth: u64,
    ) -> Result<A::MoveDatatypeLayout> {
        check_depth!(depth);
        let module = match resolver.get_module_by_id(declaring_module) {
            Err(_) | Ok(None) => bail!("Could not find module"),
            Ok(Some(m)) => m,
        };
        match (
            module.borrow().find_struct_def_by_name(name),
            module.borrow().find_enum_def_by_name(name),
        ) {
            (Some(struct_def), None) => Ok(A::MoveDatatypeLayout::Struct(
                Self::build_from_struct_definition(
                    module.borrow(),
                    struct_def,
                    type_arguments,
                    resolver,
                    depth,
                )?,
            )),
            (None, Some(enum_def)) => Ok(A::MoveDatatypeLayout::Enum(
                Self::build_from_enum_definition(
                    module.borrow(),
                    enum_def,
                    type_arguments,
                    resolver,
                    depth,
                )?,
            )),
            (Some(_), Some(_)) => panic!("Found both struct and enum with name {}", name),
            (None, None) => panic!(
                "Could not find struct/enum named {} in module {}",
                name,
                module.borrow().name()
            ),
        }
    }

    fn build_from_handle_idx(
        m: &CompiledModule,
        s: DatatypeHandleIndex,
        type_arguments: Vec<A::MoveTypeLayout>,
        resolver: &impl GetModule,
        depth: u64,
    ) -> Result<A::MoveDatatypeLayout> {
        check_depth!(depth);
        if let Some(def) = m.find_struct_def(s) {
            // declared internally
            Ok(A::MoveDatatypeLayout::Struct(
                Self::build_from_struct_definition(m, def, type_arguments, resolver, depth)?,
            ))
        } else if let Some(def) = m.find_enum_def(s) {
            Ok(A::MoveDatatypeLayout::Enum(
                Self::build_from_enum_definition(m, def, type_arguments, resolver, depth)?,
            ))
        } else {
            let handle = m.datatype_handle_at(s);
            let name = m.identifier_at(handle.name);
            let declaring_module = m.module_id_for_handle(m.module_handle_at(handle.module));
            // declared externally
            Self::build_from_name(&declaring_module, name, type_arguments, resolver, depth)
        }
    }
}
