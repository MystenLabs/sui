// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Representation of solidity types and related functions.
//! TODO: struct and function type

use anyhow::{anyhow, Context as AnyhowContext};
use itertools::Itertools;
use once_cell::sync::Lazy;
use regex::Regex;
use std::{fmt, fmt::Formatter};

use move_model::{
    model::{FunctionEnv, Parameter, QualifiedInstId, StructEnv, StructId},
    ty::{PrimitiveType, Type},
};

use crate::{attributes, context::Context};

pub(crate) const PARSE_ERR_MSG: &str = "error happens when parsing the signature";
pub(crate) const PARSE_ERR_MSG_SIMPLE_TYPE: &str = "error happens when parsing a simple type";
pub(crate) const PARSE_ERR_MSG_ARRAY_TYPE: &str = "error happens when parsing an array type";
pub(crate) const PARSE_ERR_MSG_RETURN: &str =
    "error happens when parsing the return types in the signature";
pub(crate) const PARSE_ERR_MSG_ZERO_SIZE: &str = "array with zero length specified";
pub(crate) const PARSE_ERR_STRUCT_ABI: &str =
    "error happens when parsing the abi signature for struct";

/// Represents a Solidity Signature appearing in the callable attribute.
#[derive(Debug, Clone)]
pub(crate) struct SoliditySignature {
    pub sig_name: String,
    pub para_types: Vec<(SolidityType, String, SignatureDataLocation)>,
    pub ret_types: Vec<(SolidityType, SignatureDataLocation)>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
pub(crate) enum SignatureDataLocation {
    // CallData, calldata is not supported yet
    Memory,
}

/// Represents a primitive value type.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub(crate) enum SolidityPrimitiveType {
    Bool,
    Uint(usize),
    Int(usize),
    Fixed(usize, usize),
    Ufixed(usize, usize),
    Address(bool),
}

/// Represents a Solidity type
/// TODO: struct
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
pub(crate) enum SolidityType {
    Primitive(SolidityPrimitiveType),
    Tuple(Vec<SolidityType>),
    DynamicArray(Box<SolidityType>),
    StaticArray(Box<SolidityType>, usize),
    SolidityString,
    Bytes,
    BytesStatic(usize),
    Struct(String, Vec<(usize, usize, Type, String, SolidityType)>),
}

// ================================================================================================
// Pretty print for SignatureDataLocation

impl fmt::Display for SignatureDataLocation {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use SignatureDataLocation::*;
        match self {
            // CallData => f.write_str("calldata"),
            Memory => f.write_str("memory"),
        }
    }
}

// ================================================================================================
// Pretty print for SolidityPrimitiveType

impl fmt::Display for SolidityPrimitiveType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use SolidityPrimitiveType::*;
        match self {
            Bool => f.write_str("bool"),
            Uint(n) => write!(f, "uint{}", n),
            Int(n) => write!(f, "int{}", n),
            Fixed(m, n) => write!(f, "fixed{}x{}", m, n),
            Ufixed(m, n) => write!(f, "ufixed{}x{}", m, n),
            Address(_) => f.write_str("address"),
        }
    }
}

impl SolidityPrimitiveType {
    /// Check type compatibility for primitive types
    /// TODO: int and fixed are not supported yet
    pub fn check_primitive_type_compatibility(&self, ctx: &Context, move_ty: &Type) -> bool {
        use SolidityPrimitiveType::*;
        match self {
            Bool => move_ty.is_bool(),
            Uint(i) => self.check_uint_compatibility(ctx, *i, move_ty),
            Int(i) => self.check_uint_compatibility(ctx, *i, move_ty), // current we assume int<N> in Solidity is specified in Move as a u<M> value.
            Fixed(_, _) => false,
            Ufixed(_, _) => false,
            Address(_) => move_ty.is_signer_or_address(),
        }
    }

    /// Check whether move_ty is big enough to represent a uint number
    fn check_uint_compatibility(&self, ctx: &Context, size: usize, move_ty: &Type) -> bool {
        match move_ty {
            Type::Primitive(p) => match p {
                PrimitiveType::U8 => size == 8,
                PrimitiveType::U64 => size <= 64,
                PrimitiveType::U128 => size <= 128,
                _ => false,
            },
            Type::Struct(mid, sid, _) => ctx.is_u256(mid.qualified(*sid)),
            _ => false,
        }
    }
}

// ================================================================================================
// Pretty print for SolidityType

impl fmt::Display for SolidityType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use SolidityType::*;
        match self {
            Primitive(ty) => write!(f, "{}", ty),
            Tuple(tys) => {
                let s = tys
                    .iter()
                    .map(|ref t| format!("{}", t))
                    .collect::<Vec<String>>()
                    .join(",");
                write!(f, "({})", s)
            }
            DynamicArray(ty) => write!(f, "{}[]", ty),
            StaticArray(ty, n) => write!(f, "{}[{}]", ty, n),
            SolidityString => f.write_str("string"),
            Bytes => f.write_str("bytes"),
            BytesStatic(n) => write!(f, "bytes{}", n),
            Struct(_, tys) => {
                let s = tys
                    .iter()
                    .map(|(_, _, _, _, t)| format!("{}", t))
                    .collect::<Vec<String>>()
                    .join(",");
                write!(f, "({})", s)
            }
        }
    }
}

// ================================================================================================
// Parse solidity signatures and check type compatibility

impl SolidityType {
    /// Check whether ty is a static type in the sense of serialization
    pub fn is_static(&self) -> bool {
        use SolidityType::*;
        let conjunction = |tys: &[SolidityType]| {
            tys.iter()
                .map(|t| t.is_static())
                .collect::<Vec<_>>()
                .into_iter()
                .all(|t| t)
        };
        match self {
            Primitive(_) | BytesStatic(_) => true,
            Tuple(tys) => conjunction(tys),
            StaticArray(ty, _) => ty.is_static(),
            Struct(_, tys) => {
                conjunction(&tys.iter().map(|(_, _, _, _, t)| t.clone()).collect_vec())
            }
            _ => false,
        }
    }

    /// Check whether it is a static array
    pub fn is_array_static_size(&self) -> bool {
        use SolidityType::*;
        match self {
            StaticArray(_, _) | BytesStatic(_) => true,
            SolidityString | DynamicArray(_) | Bytes => false,
            _ => panic!("wrong type"),
        }
    }

    /// Check whether it is an array
    pub fn is_array(&self) -> bool {
        use SolidityType::*;
        matches!(self, StaticArray(_, _) | DynamicArray(_))
    }

    /// Check whether a type is a value type
    pub(crate) fn is_value_type(&self) -> bool {
        use SolidityType::*;
        matches!(self, Primitive(_) | BytesStatic(_))
    }

    /// Returns the max value (bit mask) for a given type.
    pub(crate) fn max_value(&self) -> String {
        let size = self.abi_head_size(false);
        assert!(size <= 32, "unexpected type size {} for `{}`", size, self);
        let multipler = size * 8;
        format!("${{MAX_U{}}}", multipler)
    }

    /// Generate struct type using default type information
    pub(crate) fn generate_default_struct_type(ctx: &Context, st: &StructEnv<'_>) -> Self {
        let mut name = st.get_full_name_with_address();
        if let Some(i) = name.rfind(':') {
            name = name[i + 1..].to_string();
        }
        let st_id = &st.get_qualified_id().instantiate(vec![]);
        // Obtain the layout of the struct
        let layout = ctx.get_struct_layout(st_id);

        let tys = st
            .get_fields()
            .map(|field| {
                let field_type = field.get_type();
                let field_name = st.symbol_pool().string(field.get_name()).to_string();
                (field.get_offset(), field_type, field_name)
            })
            .sorted_by_key(|(offset, _, _)| *offset);
        let solidity_tys = tys
            .clone()
            .map(|(_, t, _)| {
                let bytes_flag = ctx.is_string(st_id.to_qualified_id()); // vec<u8> in String is translated into bytes
                Self::translate_from_move(ctx, &t, bytes_flag)
            })
            .collect::<Vec<_>>();
        let mut struct_tys_tuple = vec![];
        for ((offset, field_type, field_name), ty) in tys.zip(solidity_tys.iter()) {
            let (real_offset, _) = layout.offsets.get(&offset).unwrap();
            struct_tys_tuple.push((
                offset,
                *real_offset,
                field_type.clone(),
                field_name.clone(),
                ty.clone(),
            ));
        }
        SolidityType::Struct(name, struct_tys_tuple)
    }

    /// Parse type list in the signature and generate types
    fn extract_struct_type_lst(ctx: &Context, args: &str) -> anyhow::Result<Vec<SolidityType>> {
        let args_trim = args.trim();
        if args_trim.is_empty() {
            return Ok(vec![]);
        }
        let mut ret_vec = vec![];
        let paras = args_trim.split(',').collect_vec();

        for para in paras.iter() {
            let para_trim = para.trim();
            if para_trim.is_empty() {
                return Err(anyhow!(PARSE_ERR_MSG));
            }
            let para_type_str = para_trim;
            let ty = SolidityType::parse(ctx, para_type_str)?;
            ret_vec.push(ty);
        }
        Ok(ret_vec)
    }

    /// Parse the type signature and generate struct type
    pub(crate) fn parse_struct_type(
        ctx: &Context,
        sig_str: &str,
        st: &StructEnv<'_>,
    ) -> anyhow::Result<Self> {
        let tys = st
            .get_fields()
            .map(|field| {
                let field_type = field.get_type();
                let field_name = st.symbol_pool().string(field.get_name()).to_string();
                (field.get_offset(), field_type, field_name)
            })
            .sorted_by_key(|(offset, _, _)| *offset);

        static SIG_REG: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"^\s*(?P<sig_name>[a-zA-Z_$][a-zA-Z_$0-9]*)\s*\((?P<args>[^)]*)\)").unwrap()
        });

        if let Some(parsed) = SIG_REG.captures(sig_str.trim()) {
            let sig_name = parsed.name("sig_name").context(PARSE_ERR_MSG)?.as_str();
            let para_type_str = parsed.name("args").context(PARSE_ERR_MSG)?.as_str();
            let solidity_tys = SolidityType::extract_struct_type_lst(ctx, para_type_str)?;

            let st_id = &st.get_qualified_id().instantiate(vec![]);
            // Obtain the layout of the struct
            let layout = ctx.get_struct_layout(st_id);

            let mut struct_tys_tuple = vec![];
            for ((offset, field_type, field_name), ty) in tys.zip(solidity_tys.iter()) {
                let (real_offset, _) = layout.offsets.get(&offset).unwrap();
                struct_tys_tuple.push((
                    offset,
                    *real_offset,
                    field_type.clone(),
                    field_name.clone(),
                    ty.clone(),
                ));
            }
            return Ok(SolidityType::Struct(sig_name.to_string(), struct_tys_tuple));
        }
        Err(anyhow!(PARSE_ERR_STRUCT_ABI))
    }

    /// Parse a move type into a solidity type
    pub(crate) fn translate_from_move(ctx: &Context, ty: &Type, bytes_flag: bool) -> Self {
        use PrimitiveType::*;
        use Type::*;
        let generate_tuple = |tys: &Vec<Type>| {
            let s_type = tys
                .iter()
                .map(|t| Self::translate_from_move(ctx, t, bytes_flag))
                .collect::<Vec<_>>();
            SolidityType::Tuple(s_type)
        };
        match ty {
            Primitive(p) => match p {
                Bool => SolidityType::Primitive(SolidityPrimitiveType::Bool),
                U8 => SolidityType::Primitive(SolidityPrimitiveType::Uint(8)),
                U64 => SolidityType::Primitive(SolidityPrimitiveType::Uint(64)),
                U128 => SolidityType::Primitive(SolidityPrimitiveType::Uint(128)),
                Address => SolidityType::Primitive(SolidityPrimitiveType::Address(false)),
                Signer => SolidityType::Primitive(SolidityPrimitiveType::Address(false)),
                Num | Range | EventStore | U16 | U32 | U256 => {
                    panic!("unexpected field type")
                }
            },
            Vector(ety) => {
                if bytes_flag {
                    if let Primitive(U8) = **ety {
                        // translate vector<u8> to Bytes
                        return SolidityType::Bytes;
                    }
                }
                SolidityType::DynamicArray(Box::new(Self::translate_from_move(
                    ctx, ety, bytes_flag,
                )))
            }
            Tuple(tys) => generate_tuple(tys),
            Struct(mid, sid, _) => {
                if ctx.is_u256(mid.qualified(*sid)) {
                    SolidityType::Primitive(SolidityPrimitiveType::Uint(256))
                } else {
                    let struct_id = mid.qualified(*sid);
                    let struct_env = ctx.env.get_struct(struct_id);
                    if ctx.is_structs_abi(&struct_env) {
                        // Generate struct type if it is an abi_struct
                        Self::generate_default_struct_type(ctx, &struct_env)
                    } else {
                        let tys = ctx.get_field_types(mid.qualified(*sid));
                        generate_tuple(&tys) // translate into tuple type
                    }
                }
            }
            TypeParameter(_)
            | Reference(_, _)
            | Fun(_, _)
            | TypeDomain(_)
            | ResourceDomain(_, _, _)
            | Error
            | Var(_) => {
                panic!("unexpected field type")
            }
        }
    }

    /// Parse a solidity type
    pub(crate) fn parse(ctx: &Context, ty_str: &str) -> anyhow::Result<Self> {
        let trimmed_ty_str = ty_str.trim();
        if trimmed_ty_str.contains('[') {
            // array type
            SolidityType::parse_array(ctx, trimmed_ty_str)
        } else if check_simple_type_prefix(trimmed_ty_str) {
            // primitive and byte types
            SolidityType::parse_simple_type(trimmed_ty_str)
        } else {
            // Solidity identifier matching
            static RE_GENERAL_TYPE: Lazy<Regex> =
                Lazy::new(|| Regex::new(r"^[a-zA-Z_$][a-zA-Z_$0-9]*$").unwrap());
            if !RE_GENERAL_TYPE.is_match(trimmed_ty_str) {
                println!("trim:{}", trimmed_ty_str);
                let error_msg = "illegal type name";
                return Err(anyhow!(error_msg));
            }
            ctx.check_or_create_struct_abi(trimmed_ty_str)
        }
    }

    /// Parse value, bytes and string types
    fn parse_simple_type(ty_str: &str) -> anyhow::Result<Self> {
        if ty_str == "bool" {
            return Ok(SolidityType::Primitive(SolidityPrimitiveType::Bool));
        }
        if ty_str.starts_with("uint") {
            let prefix_len = "uint".len();
            if ty_str.len() > prefix_len {
                let num = ty_str[prefix_len..]
                    .parse::<usize>()
                    .context(PARSE_ERR_MSG)?;
                if check_type_int_range(num) {
                    return Ok(SolidityType::Primitive(SolidityPrimitiveType::Uint(num)));
                }
            } else {
                return Ok(SolidityType::Primitive(SolidityPrimitiveType::Uint(256)));
            }
        }
        if ty_str.starts_with("int") {
            let prefix_len = "int".len();
            if ty_str.len() > prefix_len {
                let num = ty_str[prefix_len..]
                    .parse::<usize>()
                    .context(PARSE_ERR_MSG)?;
                if check_type_int_range(num) {
                    return Ok(SolidityType::Primitive(SolidityPrimitiveType::Int(num)));
                }
            } else {
                return Ok(SolidityType::Primitive(SolidityPrimitiveType::Int(256)));
            }
        }
        if ty_str.starts_with("address") {
            let prefix_len = "address".len();
            if ty_str.len() > prefix_len {
                let address_type_array = ty_str.split_whitespace().collect_vec();
                if address_type_array.len() == 2 && address_type_array[1] == "payable" {
                    return Ok(SolidityType::Primitive(SolidityPrimitiveType::Address(
                        true,
                    )));
                }
            } else if ty_str == "address" {
                return Ok(SolidityType::Primitive(SolidityPrimitiveType::Address(
                    false,
                )));
            }
        }
        if ty_str.starts_with("fixed") {
            let prefix_len = "fixed".len();
            if ty_str.len() > prefix_len {
                let num_str = &ty_str[prefix_len..];
                let x_pos = num_str.rfind('x').context(PARSE_ERR_MSG)?;
                let num_m = num_str[0..x_pos].parse::<usize>().context(PARSE_ERR_MSG)?;
                let num_n = num_str[x_pos + 1..]
                    .parse::<usize>()
                    .context(PARSE_ERR_MSG)?;
                if check_type_int_range(num_m) && check_fixed_n_range(num_n) {
                    return Ok(SolidityType::Primitive(SolidityPrimitiveType::Fixed(
                        num_m, num_n,
                    )));
                }
            } else {
                return Ok(SolidityType::Primitive(SolidityPrimitiveType::Fixed(
                    128, 18,
                )));
            }
        }
        if ty_str.starts_with("ufixed") {
            let prefix_len = "ufixed".len();
            if ty_str.len() > prefix_len {
                let num_str = &ty_str[prefix_len..];
                let x_pos = num_str.rfind('x').context(PARSE_ERR_MSG)?;
                let num_m = num_str[0..x_pos].parse::<usize>().context(PARSE_ERR_MSG)?;
                let num_n = num_str[x_pos + 1..]
                    .parse::<usize>()
                    .context(PARSE_ERR_MSG)?;
                if check_type_int_range(num_m) && check_fixed_n_range(num_n) {
                    return Ok(SolidityType::Primitive(SolidityPrimitiveType::Ufixed(
                        num_m, num_n,
                    )));
                }
            } else {
                return Ok(SolidityType::Primitive(SolidityPrimitiveType::Ufixed(
                    128, 18,
                )));
            }
        }
        if ty_str.starts_with("bytes") {
            let prefix_len = "bytes".len();
            if ty_str.len() > prefix_len {
                let num = ty_str[prefix_len..]
                    .parse::<usize>()
                    .context(PARSE_ERR_MSG)?;
                if check_static_bytes_range(num) {
                    return Ok(SolidityType::BytesStatic(num));
                }
            } else {
                return Ok(SolidityType::Bytes);
            }
        }
        if ty_str == "string" {
            return Ok(SolidityType::SolidityString);
        }
        Err(anyhow!(PARSE_ERR_MSG_SIMPLE_TYPE))
    }

    /// Parse array types
    fn parse_array(ctx: &Context, ty_str: &str) -> anyhow::Result<Self> {
        let last_pos = ty_str.rfind('[').context(PARSE_ERR_MSG)?;
        let out_type = SolidityType::parse(ctx, &ty_str[..last_pos])?;
        let last_indice_str = &ty_str[last_pos..].trim();
        if last_indice_str.len() >= 2
            && last_indice_str.starts_with('[')
            && last_indice_str.ends_with(']')
        {
            let length_opt = last_indice_str[1..last_indice_str.len() - 1].trim();
            if !length_opt.is_empty() {
                let size = length_opt.parse::<usize>().context(PARSE_ERR_MSG)?;
                if size == 0 {
                    return Err(anyhow!(PARSE_ERR_MSG_ZERO_SIZE));
                }
                return Ok(SolidityType::StaticArray(Box::new(out_type), size));
            } else {
                return Ok(SolidityType::DynamicArray(Box::new(out_type)));
            }
        }
        Err(anyhow!(PARSE_ERR_MSG_ARRAY_TYPE))
    }

    /// Compute the data size of ty on the stack
    pub fn abi_head_size(&self, padded: bool) -> usize {
        use crate::solidity_ty::{SolidityPrimitiveType::*, SolidityType::*};
        if self.is_static() {
            match self {
                Primitive(p) => match p {
                    Bool => {
                        if padded {
                            32
                        } else {
                            1
                        }
                    }
                    Int(size) | Uint(size) | Fixed(size, _) | Ufixed(size, _) => {
                        if padded {
                            32
                        } else {
                            size / 8
                        }
                    }
                    Address(_) => {
                        if padded {
                            32
                        } else {
                            20
                        }
                    }
                },
                StaticArray(ty, size) => {
                    let mut size = ty.abi_head_size(true) * size;
                    if padded {
                        size = ((size + 31) / 32) * 32;
                    }
                    size
                }
                BytesStatic(size) => {
                    if padded {
                        32
                    } else {
                        size * 8
                    }
                }
                Tuple(tys) => abi_head_sizes_sum(tys, padded),
                Struct(_, ty_tuples) => {
                    let tys = ty_tuples
                        .iter()
                        .map(|(_, _, _, _, ty)| ty.clone())
                        .collect_vec();
                    abi_head_sizes_sum(&tys, padded)
                }
                _ => panic!("wrong types"),
            }
        } else {
            // Dynamic types
            32
        }
    }

    /// Check whether a solidity type is compatible with its corresponding move type
    /// TODO: int<M> and fixed are not supported yets
    pub(crate) fn check_type_compatibility(&self, ctx: &Context, move_ty: &Type) -> bool {
        match self {
            SolidityType::Primitive(p) => p.check_primitive_type_compatibility(ctx, move_ty),
            SolidityType::DynamicArray(array_type) | SolidityType::StaticArray(array_type, _) => {
                if let Type::Vector(ety) = move_ty {
                    array_type.check_type_compatibility(ctx, ety)
                } else {
                    false
                }
            }
            SolidityType::SolidityString => {
                // For simplifying type checking, string is only compatible with vector<u8>
                // ASCII::String is compatible with the tuple (bytes)
                /*
                if let Type::Struct(mid, sid, _) = move_ty {
                    ctx.is_string(mid.qualified(*sid))
                } else
                */
                if let Type::Vector(ety) = move_ty {
                    matches!(**ety, Type::Primitive(PrimitiveType::U8))
                } else {
                    false
                }
            }
            SolidityType::Bytes | SolidityType::BytesStatic(_) => {
                if let Type::Vector(ety) = move_ty {
                    matches!(**ety, Type::Primitive(PrimitiveType::U8))
                } else {
                    false
                }
            }
            SolidityType::Struct(struct_name, ty_tuples) => {
                if let Type::Struct(mid, sid, _) = move_ty {
                    let abi_struct_name_map_ref = ctx.abi_struct_name_map.borrow();
                    if let Some(st_id) = abi_struct_name_map_ref.get(struct_name) {
                        if *st_id == mid.qualified(*sid).instantiate(vec![]) {
                            let solidity_tys =
                                ty_tuples.iter().map(|(_, _, _, _, ty)| ty).collect_vec();
                            let field_tys = ctx.get_field_types(st_id.to_qualified_id());
                            if solidity_tys.len() == field_tys.len() {
                                for (s_ty, m_ty) in solidity_tys.iter().zip(field_tys.iter()) {
                                    if !s_ty.check_type_compatibility(ctx, m_ty) {
                                        return false;
                                    }
                                }
                                return true;
                            }
                        }
                    }
                }
                false
            }
            SolidityType::Tuple(_) => panic!("unexpected solidity type"),
        }
    }

    pub fn is_bytes_type(&self) -> bool {
        use SolidityType::*;
        matches!(self, Bytes | BytesStatic(_) | SolidityString)
    }
}

// ================================================================================================
// Pretty print for SoliditySignature

impl fmt::Display for SoliditySignature {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.selector_signature())
    }
}

impl SoliditySignature {
    /// Create a default solidity signature from a move function signature
    pub(crate) fn create_default_solidity_signature(
        ctx: &Context,
        fun: &FunctionEnv<'_>,
        storage_type: &Option<QualifiedInstId<StructId>>,
    ) -> Self {
        let fun_name = fun.symbol_pool().string(fun.get_name()).to_string();
        let mut para_type_lst = vec![];
        for (pos, Parameter(para_name, move_ty)) in fun.get_parameters().into_iter().enumerate() {
            if pos == 0 && ctx.is_storage_ref(storage_type, &move_ty) {
                // Skip the first parameter if it is a reference to contract storage.
                continue;
            }
            let solidity_ty = SolidityType::translate_from_move(ctx, &move_ty, false); // implicit mapping from a move type to a solidity type
            para_type_lst.push((
                solidity_ty,
                fun.symbol_pool().string(para_name).to_string(),
                SignatureDataLocation::Memory, // memory is used by default
            ));
        }
        let mut ret_type_lst = vec![];
        for move_ty in fun.get_return_types() {
            let solidity_ty = SolidityType::translate_from_move(ctx, &move_ty, false);
            ret_type_lst.push((solidity_ty, SignatureDataLocation::Memory));
        }

        SoliditySignature {
            sig_name: fun_name,
            para_types: para_type_lst,
            ret_types: ret_type_lst,
        }
    }

    /// Generate parameter list for computing the function selector
    fn compute_param_types(&self, param_types: &[&SolidityType]) -> String {
        let display_type_slice = |tys: &[&SolidityType]| -> String {
            tys.iter()
                .map(|t| format!("{}", t))
                .collect::<Vec<_>>()
                .join(",")
        };
        display_type_slice(param_types)
    }

    fn selector_signature(&self) -> String {
        format!(
            "{}({})",
            self.sig_name,
            self.compute_param_types(&self.para_types.iter().map(|(ty, _, _)| ty).collect_vec())
        )
    }

    /// Parse the solidity signature
    pub fn parse_into_solidity_signature(
        ctx: &Context,
        sig_str: &str,
        fun: &FunctionEnv<'_>,
        storage_type: &Option<QualifiedInstId<StructId>>,
    ) -> anyhow::Result<Self> {
        // Solidity signature matching
        static SIG_REG: Lazy<Regex> = Lazy::new(|| {
            Regex::new(
                r"^\s*(?P<sig_name>[a-zA-Z_$][a-zA-Z_$0-9]*)\s*\((?P<args>[^)]*)\)(?P<ret_ty>.*)?",
            )
            .unwrap()
        });
        if let Some(parsed) = SIG_REG.captures(sig_str.trim()) {
            let sig_name = parsed.name("sig_name").context(PARSE_ERR_MSG)?.as_str();
            let para_type_str = parsed.name("args").context(PARSE_ERR_MSG)?.as_str();
            let ret_ty_str_opt = parsed.name("ret_ty");
            let mut ret_ty = "";
            if let Some(ret_ty_str) = ret_ty_str_opt {
                let ret_ty_str_trim = ret_ty_str.as_str().trim();
                if !ret_ty_str_trim.is_empty() {
                    let mut parse_error = false;
                    if let Some(stripped_returns) = ret_ty_str_trim.strip_prefix("returns") {
                        let stripped_returns_trim = stripped_returns.trim();
                        if stripped_returns_trim.starts_with('(')
                            && stripped_returns_trim.ends_with(')')
                        {
                            ret_ty = &stripped_returns_trim[1..stripped_returns_trim.len() - 1];
                        } else {
                            parse_error = true;
                        }
                    } else {
                        parse_error = true;
                    }
                    if parse_error {
                        return Err(anyhow!(PARSE_ERR_MSG_RETURN));
                    }
                }
            }
            let mut para_names = fun
                .get_parameters()
                .iter()
                .map(|Parameter(para_name, _)| fun.symbol_pool().string(*para_name).to_string())
                .collect_vec();
            // Handle external functions where the first parameter is contract
            if attributes::is_external_fun(fun) {
                if para_names.is_empty() {
                    return Err(anyhow!(PARSE_ERR_MSG));
                }
                para_names.remove(0);
            }
            // Skip storage reference parameter.
            if !para_names.is_empty() && ctx.is_storage_ref(storage_type, &fun.get_local_type(0)) {
                para_names.remove(0);
            }
            let ret_names = vec!["".to_string(); fun.get_return_count()];
            let solidity_sig = SoliditySignature {
                sig_name: sig_name.to_string(),
                para_types: SoliditySignature::extract_para_type_str(
                    ctx,
                    para_type_str,
                    para_names,
                )?,
                ret_types: SoliditySignature::extract_para_type_str(ctx, ret_ty, ret_names)?
                    .into_iter()
                    .map(|(ty, _, loc)| (ty, loc))
                    .collect_vec(),
            };
            Ok(solidity_sig)
        } else {
            Err(anyhow!(PARSE_ERR_MSG))
        }
    }

    /// Generate pairs of solidity type and location
    fn extract_para_type_str(
        ctx: &Context,
        args: &str,
        args_name: Vec<String>,
    ) -> anyhow::Result<Vec<(SolidityType, String, SignatureDataLocation)>> {
        let args_trim = args.trim();
        if args_trim.is_empty() {
            return Ok(vec![]);
        }
        let mut ret_vec = vec![];
        let paras = args_trim.split(',').collect_vec();
        if paras.len() != args_name.len() {
            return Err(anyhow!(PARSE_ERR_MSG));
        }
        for (para, para_name) in paras.iter().zip(args_name.iter()) {
            let para_trim = para.trim();
            if para_trim.is_empty() {
                return Err(anyhow!(PARSE_ERR_MSG));
            }
            let mut data_location = SignatureDataLocation::Memory;
            let mut para_type_str = para_trim;
            let mut loc_flag = false;
            if let Some(stripped_memory) = para_trim.strip_suffix("memory") {
                let stripped_trimmed = stripped_memory.trim();
                if stripped_trimmed.ends_with(']') || stripped_trimmed.len() < stripped_memory.len()
                {
                    data_location = SignatureDataLocation::Memory;
                    para_type_str = stripped_trimmed;
                    loc_flag = true;
                } else {
                    return Err(anyhow!(PARSE_ERR_MSG));
                }
            } else if let Some(_stripped_calldata) = para_trim.strip_suffix("calldata") {
                return Err(anyhow!("calldata is not supported yet"));
            }
            let ty = SolidityType::parse(ctx, para_type_str)?;
            if loc_flag && ty.is_value_type() {
                return Err(anyhow!(
                    "data location can only be specified for array or struct types"
                ));
            }
            ret_vec.push((ty, para_name.clone(), data_location));
        }
        Ok(ret_vec)
    }

    /// Check whether the user defined solidity signature is compatible with the Move signature
    pub fn check_sig_compatibility(
        &self,
        ctx: &Context,
        fun: &FunctionEnv<'_>,
        storage_type: &Option<QualifiedInstId<StructId>>,
    ) -> bool {
        let mut para_types = fun.get_parameter_types();
        if !para_types.is_empty() && ctx.is_storage_ref(storage_type, &para_types[0]) {
            // Skip storage reference parameter.
            para_types.remove(0);
        }
        let sig_para_vec = self
            .para_types
            .iter()
            .map(|(ty, _, _)| ty)
            .collect::<Vec<_>>();
        if para_types.len() != sig_para_vec.len() {
            return false;
        }
        // Check parameter type list
        for type_pair in para_types.iter().zip(sig_para_vec.iter()) {
            let (m_ty, s_ty) = type_pair;
            if !s_ty.check_type_compatibility(ctx, m_ty) {
                return false;
            }
        }
        // Check return type list, but only if fun is not a creator.
        if !attributes::is_create_fun(fun) {
            let sig_ret_vec = self.ret_types.iter().map(|(ty, _)| ty).collect::<Vec<_>>();
            let ret_types = fun.get_return_types();
            if ret_types.len() != sig_ret_vec.len() {
                return false;
            }
            for type_pair in ret_types.iter().zip(sig_ret_vec.iter()) {
                let (m_ty, s_ty) = type_pair;
                if !s_ty.check_type_compatibility(ctx, m_ty) {
                    return false;
                }
            }
        }
        true
    }
}

fn check_simple_type_prefix(ty_str: &str) -> bool {
    /// Prefixes of value, bytes and string related types
    const SIMPLE_TYPE_PREFIX: &[&str] = &[
        "uint", "int", "ufixed", "fixed", "bool", "address", "bytes", "string",
    ];
    for prefix in SIMPLE_TYPE_PREFIX {
        if ty_str.starts_with(prefix) {
            return true;
        }
    }
    false
}

fn check_type_int_range(num: usize) -> bool {
    (8..=256).contains(&num) && num % 8 == 0
}

fn check_fixed_n_range(num: usize) -> bool {
    num <= 80
}

fn check_static_bytes_range(num: usize) -> bool {
    (1..=32).contains(&num)
}

/// Mangle a slice of solidity types.
pub(crate) fn mangle_solidity_types(tys: &[SolidityType]) -> String {
    if tys.is_empty() {
        "".to_owned()
    } else {
        format!("${}$", tys.iter().join("_"))
    }
}

/// Compute the sum of data size of tys
pub(crate) fn abi_head_sizes_sum(tys: &[SolidityType], padded: bool) -> usize {
    let size_vec = abi_head_sizes_vec(tys, padded);
    size_vec.iter().map(|(_, size)| size).sum()
}

/// Compute the data size of all types in tys
pub(crate) fn abi_head_sizes_vec(tys: &[SolidityType], padded: bool) -> Vec<(SolidityType, usize)> {
    tys.iter()
        .map(|ty_| (ty_.clone(), ty_.abi_head_size(padded)))
        .collect_vec()
}
