// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Helpers for emitting Boogie code.

// TODO(tengzhang): helpers specifically for bv types need to be refactored

use itertools::Itertools;
use num::BigUint;

use move_binary_format::file_format::TypeParameterIndex;
use move_model::{
    ast::{MemoryLabel, TempIndex, Value},
    model::{
        FieldEnv, FunctionEnv, GlobalEnv, ModuleEnv, QualifiedInstId, SpecFunId, StructEnv,
        StructId, SCRIPT_MODULE_NAME,
    },
    pragmas::INTRINSIC_TYPE_MAP,
    symbol::Symbol,
    ty::{PrimitiveType, Type},
};
use move_stackless_bytecode::{function_target::FunctionTarget, stackless_bytecode::Constant};

use crate::options::BoogieOptions;

pub const MAX_MAKE_VEC_ARGS: usize = 4;

/// Return boogie name of given module.
pub fn boogie_module_name(env: &ModuleEnv<'_>) -> String {
    let mod_name = env.get_name();
    let mod_sym = env.symbol_pool().string(mod_name.name());
    if mod_sym.as_str() == SCRIPT_MODULE_NAME {
        // <SELF> is not accepted by boogie as a symbol
        "#SELF#".to_string()
    } else {
        // qualify module by address.
        format!("{}_{}", mod_name.addr().to_str_radix(16), mod_sym)
    }
}

/// Return boogie name of given structure.
pub fn boogie_struct_name(struct_env: &StructEnv<'_>, inst: &[Type]) -> String {
    boogie_struct_name_bv(struct_env, inst, false)
}

pub fn boogie_struct_name_bv(struct_env: &StructEnv<'_>, inst: &[Type], bv_flag: bool) -> String {
    if struct_env.is_intrinsic_of(INTRINSIC_TYPE_MAP) {
        // Map to the theory type representation, which is `Table int V`. The key
        // is encoded as an integer to avoid extensionality problems, and to support
        // $Mutation paths, which are sequences of ints.
        let env = struct_env.module_env.env;
        let type_fun = if bv_flag { boogie_bv_type } else { boogie_type };
        format!("Table int ({})", type_fun(env, &inst[1]))
    } else {
        format!(
            "${}_{}{}",
            boogie_module_name(&struct_env.module_env),
            struct_env.get_name().display(struct_env.symbol_pool()),
            boogie_inst_suffix(struct_env.module_env.env, inst)
        )
    }
}

/// Return field selector for given field.
pub fn boogie_field_sel(field_env: &FieldEnv<'_>, inst: &[Type]) -> String {
    let struct_env = &field_env.struct_env;
    format!(
        "${}#{}",
        field_env.get_name().display(struct_env.symbol_pool()),
        boogie_struct_name(struct_env, inst)
    )
}

/// Return field selector for given field.
pub fn boogie_field_update(field_env: &FieldEnv<'_>, inst: &[Type]) -> String {
    let struct_env = &field_env.struct_env;
    let suffix = boogie_type_suffix_for_struct(struct_env, inst, false);
    format!(
        "$Update'{}'_{}",
        suffix,
        field_env.get_name().display(struct_env.symbol_pool()),
    )
}

/// Return boogie name of given function.
pub fn boogie_function_name(fun_env: &FunctionEnv<'_>, inst: &[Type]) -> String {
    format!(
        "${}_{}{}",
        boogie_module_name(&fun_env.module_env),
        fun_env.get_name().display(fun_env.symbol_pool()),
        boogie_inst_suffix(fun_env.module_env.env, inst)
    )
}

/// Return boogie name of given function
/// Currently bv_flag is used when generating vector functions
pub fn boogie_function_bv_name(
    fun_env: &FunctionEnv<'_>,
    inst: &[Type],
    bv_flag: &[bool],
) -> String {
    format!(
        "${}_{}{}",
        boogie_module_name(&fun_env.module_env),
        fun_env.get_name().display(fun_env.symbol_pool()),
        boogie_inst_suffix_bv(fun_env.module_env.env, inst, bv_flag)
    )
}

/// Return boogie name of given spec var.
pub fn boogie_spec_var_name(
    module_env: &ModuleEnv<'_>,
    name: Symbol,
    inst: &[Type],
    memory_label: &Option<MemoryLabel>,
) -> String {
    format!(
        "${}_{}{}{}",
        boogie_module_name(module_env),
        name.display(module_env.symbol_pool()),
        boogie_inst_suffix(module_env.env, inst),
        boogie_memory_label(memory_label)
    )
}

/// Return boogie name of given spec function.
pub fn boogie_spec_fun_name(
    env: &ModuleEnv<'_>,
    id: SpecFunId,
    inst: &[Type],
    bv_flag: bool,
) -> String {
    let decl = env.get_spec_fun(id);
    let pos = env
        .get_spec_funs_of_name(decl.name)
        .position(|(overload_id, _)| &id == overload_id)
        .expect("spec fun env inconsistent");
    let overload_qualifier = if pos > 0 {
        format!("_{}", pos)
    } else {
        "".to_string()
    };
    let mut suffix = boogie_inst_suffix_bv(env.env, inst, &[bv_flag]);
    if env.is_table() {
        assert_eq!(inst.len(), 2);
        suffix = boogie_inst_suffix_bv_pair(env.env, inst, &[false, bv_flag]);
    };
    format!(
        "${}_{}{}{}",
        boogie_module_name(env),
        decl.name.display(env.symbol_pool()),
        overload_qualifier,
        suffix
    )
}

/// Return boogie name for function representing a lifted `some` expression.
pub fn boogie_choice_fun_name(id: usize) -> String {
    format!("$choice_{}", id)
}

/// Creates the name of the resource memory domain for any function for the given struct.
/// This variable represents a local variable of the Boogie translation of this function.
pub fn boogie_modifies_memory_name(env: &GlobalEnv, memory: &QualifiedInstId<StructId>) -> String {
    let struct_env = &env.get_struct_qid(memory.to_qualified_id());
    format!("{}_$modifies", boogie_struct_name(struct_env, &memory.inst))
}

/// Creates the name of the resource memory for the given struct.
pub fn boogie_resource_memory_name(
    env: &GlobalEnv,
    memory: &QualifiedInstId<StructId>,
    memory_label: &Option<MemoryLabel>,
) -> String {
    let struct_env = env.get_struct_qid(memory.to_qualified_id());
    format!(
        "{}_$memory{}",
        boogie_struct_name(&struct_env, &memory.inst),
        boogie_memory_label(memory_label)
    )
}

/// Creates a string for a memory label.
fn boogie_memory_label(memory_label: &Option<MemoryLabel>) -> String {
    if let Some(l) = memory_label {
        format!("#{}", l.as_usize())
    } else {
        "".to_string()
    }
}

/// Creates a vector from the given list of arguments.
pub fn boogie_make_vec_from_strings(args: &[String]) -> String {
    if args.is_empty() {
        "EmptyVec()".to_string()
    } else {
        let mut make = "".to_owned();
        let mut at = 0;
        loop {
            let n = usize::min(args.len() - at, MAX_MAKE_VEC_ARGS);
            let m = format!("MakeVec{}({})", n, args[at..at + n].iter().join(", "));
            make = if make.is_empty() {
                m
            } else {
                format!("ConcatVec({}, {})", make, m)
            };
            at += n;
            if at >= args.len() {
                break;
            }
        }
        make
    }
}

/// Return boogie type for a local with given signature token.
pub fn boogie_type(env: &GlobalEnv, ty: &Type) -> String {
    use PrimitiveType::*;
    use Type::*;
    match ty {
        Primitive(p) => match p {
            U8 | U16 | U32 | U64 | U128 | U256 | Num | Address => "int".to_string(),
            Signer => "$signer".to_string(),
            Bool => "bool".to_string(),
            Range | EventStore => panic!("unexpected type"),
        },
        Vector(et) => format!("Vec ({})", boogie_type(env, et)),
        Struct(mid, sid, inst) => boogie_struct_name(&env.get_module(*mid).into_struct(*sid), inst),
        Reference(_, bt) => format!("$Mutation ({})", boogie_type(env, bt)),
        TypeParameter(idx) => boogie_type_param(env, *idx),
        Fun(..) | Tuple(..) | TypeDomain(..) | ResourceDomain(..) | Error | Var(..) => {
            format!("<<unsupported: {:?}>>", ty)
        }
    }
}

/// Return boogie type for a local with given signature token.
/// TODO(tengzhang): combine with boogie_type later
pub fn boogie_bv_type(env: &GlobalEnv, ty: &Type) -> String {
    use PrimitiveType::*;
    use Type::*;
    match ty {
        Primitive(p) => match p {
            U8 => "bv8".to_string(),
            U16 => "bv16".to_string(),
            U32 => "bv32".to_string(),
            U64 => "bv64".to_string(),
            U128 => "bv128".to_string(),
            U256 => "bv256".to_string(),
            Address => "int".to_string(),
            Signer => "$signer".to_string(),
            Bool => "bool".to_string(),
            Range | EventStore => panic!("unexpected type"),
            Num => "<<num is not unsupported here>>".to_string(),
        },
        Vector(et) => format!("Vec ({})", boogie_bv_type(env, et)),
        Struct(mid, sid, inst) => {
            boogie_struct_name_bv(&env.get_module(*mid).into_struct(*sid), inst, true)
        }
        Reference(_, bt) => format!("$Mutation ({})", boogie_bv_type(env, bt)),
        TypeParameter(idx) => boogie_type_param(env, *idx),
        Fun(..) | Tuple(..) | TypeDomain(..) | ResourceDomain(..) | Error | Var(..) => {
            format!("<<unsupported: {:?}>>", ty)
        }
    }
}

pub fn boogie_type_param(_env: &GlobalEnv, idx: u16) -> String {
    format!("#{}", idx)
}

pub fn boogie_temp(env: &GlobalEnv, ty: &Type, instance: usize, bv_flag: bool) -> String {
    boogie_temp_from_suffix(env, &boogie_type_suffix_bv(env, ty, bv_flag), instance)
}

pub fn boogie_temp_from_suffix(_env: &GlobalEnv, suffix: &str, instance: usize) -> String {
    format!("$temp_{}'{}'", instance, suffix)
}

/// Generate number literals that may comes with a bv suffix in the boogie code
pub fn boogie_num_literal(num: &String, base: usize, bv_flag: bool) -> String {
    if bv_flag {
        format!("{}bv{}", num, base)
    } else {
        num.clone()
    }
}

pub fn boogie_num_type_string(num: &str, bv_flag: bool) -> String {
    let pre = if bv_flag { "bv" } else { "u" };
    [pre, num].join("")
}

pub fn boogie_num_type_string_capital(num: &str, bv_flag: bool) -> String {
    let pre = if bv_flag { "Bv" } else { "U" };
    [pre, num].join("")
}

pub fn boogie_num_type_base(ty: &Type) -> String {
    use PrimitiveType::*;
    use Type::*;
    match ty {
        Primitive(p) => match p {
            U8 => "8".to_string(),
            U16 => "16".to_string(),
            U32 => "32".to_string(),
            U64 => "64".to_string(),
            U128 => "128".to_string(),
            U256 => "256".to_string(),
            Num => "<<num is not unsupported here>>".to_string(),
            _ => format!("<<unsupported {:?}>>", ty),
        },
        _ => format!("<<unsupported {:?}>>", ty),
    }
}

/// Returns the suffix to specialize a name for the given type instance.
pub fn boogie_type_suffix_bv(env: &GlobalEnv, ty: &Type, bv_flag: bool) -> String {
    use PrimitiveType::*;
    use Type::*;

    match ty {
        Primitive(p) => match p {
            U8 => boogie_num_type_string("8", bv_flag),
            U16 => boogie_num_type_string("16", bv_flag),
            U32 => boogie_num_type_string("32", bv_flag),
            U64 => boogie_num_type_string("64", bv_flag),
            U128 => boogie_num_type_string("128", bv_flag),
            U256 => boogie_num_type_string("256", bv_flag),
            Num => {
                if bv_flag {
                    "<<num is not unsupported here>>".to_string()
                } else {
                    "num".to_string()
                }
            }
            Address => "address".to_string(),
            Signer => "signer".to_string(),
            Bool => "bool".to_string(),
            Range => "range".to_string(),
            EventStore => format!("<<unsupported {:?}>>", ty),
        },
        Vector(et) => format!(
            "vec{}",
            boogie_inst_suffix_bv(env, &[et.as_ref().to_owned()], &[bv_flag])
        ),
        Struct(mid, sid, inst) => {
            boogie_type_suffix_for_struct(&env.get_module(*mid).into_struct(*sid), inst, bv_flag)
        }
        TypeParameter(idx) => boogie_type_param(env, *idx),
        Fun(..) | Tuple(..) | TypeDomain(..) | ResourceDomain(..) | Error | Var(..)
        | Reference(..) => format!("<<unsupported {:?}>>", ty),
    }
}

/// Return the suffix to specialize a name for the given type instance.
pub fn boogie_type_suffix(env: &GlobalEnv, ty: &Type) -> String {
    boogie_type_suffix_bv(env, ty, false)
}

pub fn boogie_type_suffix_for_struct(
    struct_env: &StructEnv<'_>,
    inst: &[Type],
    bv_flag: bool,
) -> String {
    if struct_env.is_intrinsic_of(INTRINSIC_TYPE_MAP) {
        format!(
            "${}_{}{}",
            boogie_module_name(&struct_env.module_env),
            struct_env.get_name().display(struct_env.symbol_pool()),
            boogie_inst_suffix_bv_pair(struct_env.module_env.env, inst, &[false, bv_flag])
        )
    } else {
        boogie_struct_name(struct_env, inst)
    }
}

/// Generate suffix after instantiation of type parameters
pub fn boogie_inst_suffix_bv(env: &GlobalEnv, inst: &[Type], bv_flag: &[bool]) -> String {
    if inst.is_empty() {
        "".to_owned()
    } else {
        let suffix = if bv_flag.len() == 1 {
            inst.iter()
                .map(|ty| boogie_type_suffix_bv(env, ty, bv_flag[0]))
                .join("_")
        } else {
            assert_eq!(inst.len(), bv_flag.len());
            inst.iter()
                .zip(bv_flag.iter())
                .map(|(ty, flag)| boogie_type_suffix_bv(env, ty, *flag))
                .join("_")
        };
        format!("'{}'", suffix)
    }
}

pub fn boogie_inst_suffix_bv_pair(env: &GlobalEnv, inst: &[Type], bv_flag: &[bool]) -> String {
    if inst.is_empty() {
        "".to_owned()
    } else {
        assert_eq!(inst.len(), bv_flag.len());
        format!(
            "'{}'",
            inst.iter()
                .zip(bv_flag.iter())
                .map(|(ty, flag)| boogie_type_suffix_bv(env, ty, *flag))
                .join("_")
        )
    }
}

pub fn boogie_inst_suffix(env: &GlobalEnv, inst: &[Type]) -> String {
    if inst.is_empty() {
        "".to_owned()
    } else {
        format!(
            "'{}'",
            inst.iter().map(|ty| boogie_type_suffix(env, ty)).join("_")
        )
    }
}

pub fn boogie_equality_for_type(env: &GlobalEnv, eq: bool, ty: &Type, bv_flag: bool) -> String {
    format!(
        "{}'{}'",
        if eq { "$IsEqual" } else { "!$IsEqual" },
        boogie_type_suffix_bv(env, ty, bv_flag)
    )
}

/// Create boogie well-formed boolean expression
/// TODO(tengzhang): combine with boogie_well_formed_expr
pub fn boogie_well_formed_expr_bv(env: &GlobalEnv, name: &str, ty: &Type, bv_flag: bool) -> String {
    let target = if ty.is_reference() {
        format!("$Dereference({})", name)
    } else {
        name.to_owned()
    };
    let suffix = boogie_type_suffix_bv(env, ty.skip_reference(), bv_flag);
    format!("$IsValid'{}'({})", suffix, target)
}

/// Create boogie well-formed boolean expression.
pub fn boogie_well_formed_expr(env: &GlobalEnv, name: &str, ty: &Type) -> String {
    let target = if ty.is_reference() {
        format!("$Dereference({})", name)
    } else {
        name.to_owned()
    };
    let suffix = boogie_type_suffix(env, ty.skip_reference());
    format!("$IsValid'{}'({})", suffix, target)
}

/// Create boogie well-formed check. The result will be either an empty string or a
/// newline-terminated assume statement.
pub fn boogie_well_formed_check(env: &GlobalEnv, name: &str, ty: &Type, bv_flag: bool) -> String {
    let expr = boogie_well_formed_expr_bv(env, name, ty, bv_flag);
    if !expr.is_empty() {
        format!("assume {};", expr)
    } else {
        "".to_string()
    }
}

/// Create boogie global variable with type constraint. No references allowed.
pub fn boogie_declare_global(env: &GlobalEnv, name: &str, ty: &Type) -> String {
    assert!(!ty.is_reference());
    format!(
        "var {} : {} where {};",
        name,
        boogie_type(env, ty),
        // TODO: boogie crash boogie_well_formed_expr(env, name, ty)
        // boogie_well_formed_expr(env, name, ty)"
        "true"
    )
}

pub fn boogie_byte_blob(_options: &BoogieOptions, val: &[u8], bv_flag: bool) -> String {
    let val_suffix = if bv_flag { "bv8" } else { "" };
    let suffix = if bv_flag { "bv8" } else { "u8" };
    let args = val
        .iter()
        .map(|v| format!("{}{}", *v, val_suffix))
        .collect_vec();
    if args.is_empty() {
        format!("$EmptyVec'{}'()", suffix)
    } else {
        boogie_make_vec_from_strings(&args)
    }
}

pub fn boogie_address_blob(_options: &BoogieOptions, val: &[BigUint]) -> String {
    let args = val.iter().map(|v| format!("{}", *v)).collect_vec();
    if args.is_empty() {
        "$EmptyVec'address'()".to_string()
    } else {
        boogie_make_vec_from_strings(&args)
    }
}

/// Generate vectors for constant values
/// TODO(tengzhang): add support for bv types
pub fn boogie_constant_blob(_options: &BoogieOptions, val: &[Constant]) -> String {
    let args = val
        .iter()
        .map(|v| boogie_constant(_options, v))
        .collect_vec();
    if args.is_empty() {
        "EmptyVec()".to_string()
    } else {
        boogie_make_vec_from_strings(&args)
    }
}

pub fn boogie_constant(_options: &BoogieOptions, val: &Constant) -> String {
    match val {
        Constant::Bool(true) => "true".to_string(),
        Constant::Bool(false) => "false".to_string(),
        Constant::U8(num) => num.to_string(),
        Constant::U64(num) => num.to_string(),
        Constant::U128(num) => num.to_string(),
        Constant::U256(num) => num.to_string(),
        Constant::Address(v) => v.to_string(),
        Constant::ByteArray(v) => boogie_byte_blob(_options, v, false),
        Constant::AddressArray(v) => boogie_address_blob(_options, v),
        Constant::Vector(vec) => boogie_make_vec_from_strings(
            &vec.iter()
                .map(|v| boogie_constant(_options, v))
                .collect_vec(),
        ),
        Constant::U16(num) => num.to_string(),
        Constant::U32(num) => num.to_string(),
    }
}

pub fn boogie_value_blob(_options: &BoogieOptions, val: &[Value]) -> String {
    let args = val.iter().map(|v| boogie_value(_options, v)).collect_vec();
    if args.is_empty() {
        "EmptyVec()".to_string()
    } else {
        boogie_make_vec_from_strings(&args)
    }
}

pub fn boogie_value(_options: &BoogieOptions, val: &Value) -> String {
    match val {
        Value::Bool(true) => "true".to_string(),
        Value::Bool(false) => "false".to_string(),
        Value::Number(num) => num.to_string(),
        Value::Address(v) => v.to_string(),
        Value::ByteArray(v) => boogie_byte_blob(_options, v, false),
        Value::AddressArray(v) => boogie_address_blob(_options, v),
        Value::Vector(vec) => boogie_make_vec_from_strings(
            &vec.iter().map(|v| boogie_value(_options, v)).collect_vec(),
        ),
    }
}

/// Construct a statement to debug track a local based on the Boogie attribute approach.
pub fn boogie_debug_track_local(
    fun_target: &FunctionTarget<'_>,
    origin_idx: TempIndex,
    idx: TempIndex,
    ty: &Type,
    bv_flag: bool,
) -> String {
    boogie_debug_track(fun_target, "$track_local", origin_idx, idx, ty, bv_flag)
}

fn boogie_debug_track(
    fun_target: &FunctionTarget<'_>,
    track_tag: &str,
    tracked_idx: usize,
    idx: TempIndex,
    ty: &Type,
    bv_flag: bool,
) -> String {
    let fun_def_idx = fun_target.func_env.get_def_idx();
    let value = format!("$t{}", idx);
    if ty.is_reference() {
        let temp_name = boogie_temp(fun_target.global_env(), ty.skip_reference(), 0, bv_flag);
        format!(
            "{} := $Dereference({});\n\
             assume {{:print \"{}({},{},{}):\", {}}} {} == {};",
            temp_name,
            value,
            track_tag,
            fun_target.func_env.module_env.get_id().to_usize(),
            fun_def_idx,
            tracked_idx,
            temp_name,
            temp_name,
            temp_name
        )
    } else {
        format!(
            "assume {{:print \"{}({},{},{}):\", {}}} {} == {};",
            track_tag,
            fun_target.func_env.module_env.get_id().to_usize(),
            fun_def_idx,
            tracked_idx,
            value,
            value,
            value
        )
    }
}

/// Construct a statement to debug track an abort.
pub fn boogie_debug_track_abort(fun_target: &FunctionTarget<'_>, abort_code: &str) -> String {
    let fun_def_idx = fun_target.func_env.get_def_idx();
    format!(
        "assume {{:print \"$track_abort({},{}):\", {}}} {} == {};",
        fun_target.func_env.module_env.get_id().to_usize(),
        fun_def_idx,
        abort_code,
        abort_code,
        abort_code,
    )
}

/// Construct a statement to debug track a return value.
pub fn boogie_debug_track_return(
    fun_target: &FunctionTarget<'_>,
    ret_idx: usize,
    idx: TempIndex,
    ty: &Type,
    bv_flag: bool,
) -> String {
    boogie_debug_track(fun_target, "$track_return", ret_idx, idx, ty, bv_flag)
}

pub enum TypeIdentToken {
    Char(u8),
    Variable(String),
}

impl TypeIdentToken {
    pub fn make(name: &str) -> Vec<TypeIdentToken> {
        name.as_bytes()
            .iter()
            .map(|c| TypeIdentToken::Char(*c))
            .collect()
    }

    pub fn join(sep: &str, mut pieces: Vec<Vec<TypeIdentToken>>) -> Vec<TypeIdentToken> {
        if pieces.is_empty() {
            return vec![];
        }

        pieces.reverse();
        let mut tokens = pieces.pop().unwrap();
        while !pieces.is_empty() {
            tokens.extend(Self::make(sep));
            tokens.extend(pieces.pop().unwrap());
        }
        tokens
    }

    pub fn convert_to_bytes(tokens: Vec<TypeIdentToken>) -> String {
        fn get_char_array(tokens: &[TypeIdentToken], start: usize, end: usize) -> String {
            let elements = (start..end)
                .map(|k| {
                    format!(
                        "[{} := {}]",
                        k - start,
                        match &tokens[k] {
                            TypeIdentToken::Char(c) => *c,
                            TypeIdentToken::Variable(_) => unreachable!(),
                        }
                    )
                })
                .join("");
            format!("Vec(DefaultVecMap(){}, {})", elements, end - start)
        }

        // construct all the segments
        let mut segments = vec![];

        let mut char_seq_start = None;
        for (i, token) in tokens.iter().enumerate() {
            match token {
                TypeIdentToken::Char(_) => {
                    if char_seq_start.is_none() {
                        char_seq_start = Some(i);
                    }
                }
                TypeIdentToken::Variable(name) => {
                    if let Some(start) = &char_seq_start {
                        segments.push(get_char_array(&tokens, *start, i));
                    };
                    char_seq_start = None;
                    segments.push(name.clone());
                }
            }
        }
        if let Some(start) = char_seq_start {
            segments.push(get_char_array(&tokens, start, tokens.len()));
        }

        // concat the segments
        if segments.is_empty() {
            return String::new();
        }

        segments.reverse();
        let mut cursor = segments.pop().unwrap();
        while !segments.is_empty() {
            let next = segments.pop().unwrap();
            cursor = format!("ConcatVec({}, {})", cursor, next);
        }
        cursor
    }
}

/// A formatter for address
pub struct AddressFormatter {
    /// whether the `0x` prefix is needed
    pub prefix: bool,
    /// whether to include leading zeros
    pub full_length: bool,
    /// whether to capitalize the hex repr
    pub capitalized: bool,
}

impl AddressFormatter {
    pub fn format(&self, addr: &BigUint) -> String {
        let result = addr.to_str_radix(16);
        // into correct length
        let result = if self.full_length {
            format!("{:0>32}", result)
        } else {
            result
        };
        // into correct case
        let result = if self.capitalized {
            result.to_uppercase()
        } else {
            result
        };
        // with or without prefix
        if self.prefix {
            format!("0x{}", result)
        } else {
            result
        }
    }
}

fn type_name_to_ident_tokens(
    env: &GlobalEnv,
    ty: &Type,
    formatter: &AddressFormatter,
) -> Vec<TypeIdentToken> {
    match ty {
        Type::Primitive(PrimitiveType::Bool) => TypeIdentToken::make("bool"),
        Type::Primitive(PrimitiveType::U8) => TypeIdentToken::make("u8"),
        Type::Primitive(PrimitiveType::U16) => TypeIdentToken::make("u16"),
        Type::Primitive(PrimitiveType::U32) => TypeIdentToken::make("u32"),
        Type::Primitive(PrimitiveType::U64) => TypeIdentToken::make("u64"),
        Type::Primitive(PrimitiveType::U128) => TypeIdentToken::make("u128"),
        Type::Primitive(PrimitiveType::U256) => TypeIdentToken::make("u256"),
        Type::Primitive(PrimitiveType::Address) => TypeIdentToken::make("address"),
        Type::Primitive(PrimitiveType::Signer) => TypeIdentToken::make("signer"),
        Type::Vector(element) => {
            let mut tokens = TypeIdentToken::make("vector<");
            tokens.extend(type_name_to_ident_tokens(env, element, formatter));
            tokens.extend(TypeIdentToken::make(">"));
            tokens
        }
        Type::Struct(mid, sid, ty_args) => {
            let module_env = env.get_module(*mid);
            let struct_env = module_env.get_struct(*sid);
            let type_name = format!(
                "{}::{}::{}",
                formatter.format(module_env.get_name().addr()),
                module_env
                    .get_name()
                    .name()
                    .display(module_env.symbol_pool()),
                struct_env.get_name().display(module_env.symbol_pool())
            );
            let mut tokens = TypeIdentToken::make(&type_name);
            if !ty_args.is_empty() {
                tokens.extend(TypeIdentToken::make("<"));
                let ty_args_tokens = ty_args
                    .iter()
                    .map(|t| type_name_to_ident_tokens(env, t, formatter))
                    .collect();
                tokens.extend(TypeIdentToken::join(", ", ty_args_tokens));
                tokens.extend(TypeIdentToken::make(">"));
            }
            tokens
        }
        Type::TypeParameter(idx) => {
            vec![TypeIdentToken::Variable(format!(
                "$TypeName(#{}_info)",
                *idx
            ))]
        }
        // move types that are not allowed
        Type::Reference(..) | Type::Tuple(..) => {
            unreachable!("Prohibited move type in type_name call");
        }
        // spec only types
        Type::Primitive(PrimitiveType::Num)
        | Type::Primitive(PrimitiveType::Range)
        | Type::Primitive(PrimitiveType::EventStore)
        | Type::Fun(..)
        | Type::TypeDomain(..)
        | Type::ResourceDomain(..) => {
            unreachable!("Unexpected spec-only type in type_name call");
        }
        // temporary types
        Type::Error | Type::Var(..) => {
            unreachable!("Unexpected temporary type in type_name call");
        }
    }
}

/// Convert a type name into a format that can be recognized by Boogie
///
/// The `stdlib` bool flag represents whether this type name is intended for
/// - true  --> `std::type_name` and
/// - false --> `ext::type_info`.
/// TODO(mengxu): the above is a very hacky, we need a better way to differentiate
pub fn boogie_reflection_type_name(env: &GlobalEnv, ty: &Type, stdlib: bool) -> String {
    let formatter = if stdlib {
        AddressFormatter {
            prefix: false,
            full_length: true,
            capitalized: false,
        }
    } else {
        AddressFormatter {
            prefix: true,
            full_length: false,
            capitalized: false,
        }
    };
    let bytes = TypeIdentToken::convert_to_bytes(type_name_to_ident_tokens(env, ty, &formatter));
    if stdlib {
        format!(
            "${}_type_name_TypeName(${}_ascii_String({}))",
            env.get_stdlib_address(),
            env.get_stdlib_address(),
            bytes
        )
    } else {
        format!("${}_string_String({})", env.get_stdlib_address(), bytes)
    }
}

enum TypeInfoPack {
    Struct(BigUint, String, String),
    Symbolic(TypeParameterIndex),
}

fn type_name_to_info_pack(env: &GlobalEnv, ty: &Type) -> Option<TypeInfoPack> {
    match ty {
        Type::Struct(mid, sid, _) => {
            let module_env = env.get_module(*mid);
            let struct_env = module_env.get_struct(*sid);
            let module_name = module_env.get_name();
            Some(TypeInfoPack::Struct(
                module_name.addr().clone(),
                module_name
                    .name()
                    .display(module_env.symbol_pool())
                    .to_string(),
                struct_env
                    .get_name()
                    .display(module_env.symbol_pool())
                    .to_string(),
            ))
        }
        Type::TypeParameter(idx) => Some(TypeInfoPack::Symbolic(*idx)),
        // move types that will cause an error
        Type::Primitive(PrimitiveType::Bool)
        | Type::Primitive(PrimitiveType::U8)
        | Type::Primitive(PrimitiveType::U16)
        | Type::Primitive(PrimitiveType::U32)
        | Type::Primitive(PrimitiveType::U64)
        | Type::Primitive(PrimitiveType::U128)
        | Type::Primitive(PrimitiveType::U256)
        | Type::Primitive(PrimitiveType::Address)
        | Type::Primitive(PrimitiveType::Signer)
        | Type::Vector(_) => None,
        // move types that are not allowed
        Type::Reference(..) | Type::Tuple(..) => {
            unreachable!("Prohibited move type in type_name call");
        }
        // spec only types
        Type::Primitive(PrimitiveType::Num)
        | Type::Primitive(PrimitiveType::Range)
        | Type::Primitive(PrimitiveType::EventStore)
        | Type::Fun(..)
        | Type::TypeDomain(..)
        | Type::ResourceDomain(..) => {
            unreachable!("Unexpected spec-only type in type_name call");
        }
        // temporary types
        Type::Error | Type::Var(..) => {
            unreachable!("Unexpected temporary type in type_name call");
        }
    }
}

/// Convert a type info into a format that can be recognized by Boogie
pub fn boogie_reflection_type_info(env: &GlobalEnv, ty: &Type) -> (String, String) {
    fn get_symbol_is_struct(idx: TypeParameterIndex) -> String {
        format!("is#$TypeParamStruct(#{}_info)", idx)
    }
    fn get_symbol_account_address(idx: TypeParameterIndex) -> String {
        format!("a#$TypeParamStruct(#{}_info)", idx)
    }
    fn get_symbol_module_name(idx: TypeParameterIndex) -> String {
        format!("m#$TypeParamStruct(#{}_info)", idx)
    }
    fn get_symbol_struct_name(idx: TypeParameterIndex) -> String {
        format!("s#$TypeParamStruct(#{}_info)", idx)
    }

    let extlib_address = env.get_extlib_address();
    match type_name_to_info_pack(env, ty) {
        None => (
            "false".to_string(),
            format!(
                "${}_type_info_TypeInfo(0, EmptyVec(), EmptyVec())",
                extlib_address
            ),
        ),
        Some(TypeInfoPack::Struct(addr, module_name, struct_name)) => {
            let module_repr = TypeIdentToken::convert_to_bytes(TypeIdentToken::make(&module_name));
            let struct_repr = TypeIdentToken::convert_to_bytes(TypeIdentToken::make(&struct_name));
            (
                "true".to_string(),
                format!(
                    "${}_type_info_TypeInfo({}, {}, {})",
                    extlib_address, addr, module_repr, struct_repr
                ),
            )
        }
        Some(TypeInfoPack::Symbolic(idx)) => (
            get_symbol_is_struct(idx),
            format!(
                "${}_type_info_TypeInfo({}, {}, {})",
                extlib_address,
                get_symbol_account_address(idx),
                get_symbol_module_name(idx),
                get_symbol_struct_name(idx)
            ),
        ),
    }
}

/// Encode the test on whether a type is a struct in a format that can be recognized by Boogie
pub fn boogie_reflection_type_is_struct(env: &GlobalEnv, ty: &Type) -> String {
    match type_name_to_info_pack(env, ty) {
        None => "false".to_string(),
        Some(TypeInfoPack::Struct(..)) => "true".to_string(),
        Some(TypeInfoPack::Symbolic(idx)) => format!("is#$TypeParamStruct(#{}_info)", idx),
    }
}
