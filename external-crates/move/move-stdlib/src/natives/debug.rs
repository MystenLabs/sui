// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::natives::helpers::make_module_natives;
use move_binary_format::errors::PartialVMResult;
use move_core_types::{account_address::AccountAddress, gas_algebra::InternalGas};
use move_vm_runtime::native_functions::{NativeContext, NativeFunction};
#[allow(unused_imports)]
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    pop_arg,
    values::{Reference, Value},
};
use smallvec::smallvec;
use std::{collections::VecDeque, sync::Arc};

/***************************************************************************************************
 * native fun print
 *
 *   gas cost: base_cost
 **************************************************************************************************/
#[derive(Debug, Clone)]
pub struct PrintGasParameters {
    pub base_cost: InternalGas,
}

#[inline]
fn native_print(
    gas_params: &PrintGasParameters,
    _context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
    _move_std_addr: AccountAddress,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 1);

    let _val = args.pop_back().unwrap();
    let _ty = ty_args.pop().unwrap();

    // No-op if the feature flag is not present.
    #[cfg(feature = "testing")]
    {
        let canonical = false;
        let single_line = false;
        let include_int_types = false;

        let mut out = "[debug] ".to_string();
        let val = _val.value_as::<Reference>()?.read_ref()?;

        testing::print_value(
            _context,
            &mut out,
            val,
            _ty,
            &_move_std_addr,
            0,
            canonical,
            single_line,
            include_int_types,
        )?;
        println!("{}", out);
    }

    Ok(NativeResult::ok(gas_params.base_cost, smallvec![]))
}

pub fn make_native_print(
    gas_params: PrintGasParameters,
    move_std_addr: AccountAddress,
) -> NativeFunction {
    Arc::new(
        move |context, ty_args, args| -> PartialVMResult<NativeResult> {
            native_print(&gas_params, context, ty_args, args, move_std_addr)
        },
    )
}

/***************************************************************************************************
 * native fun print_stack_trace
 *
 *   gas cost: base_cost
 **************************************************************************************************/
#[derive(Debug, Clone)]
pub struct PrintStackTraceGasParameters {
    pub base_cost: InternalGas,
}

#[allow(unused_variables)]
#[inline]
fn native_print_stack_trace(
    gas_params: &PrintStackTraceGasParameters,
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.is_empty());

    #[cfg(feature = "testing")]
    {
        let mut s = String::new();
        context.print_stack_trace(&mut s)?;
        println!("{}", s);
    }

    Ok(NativeResult::ok(gas_params.base_cost, smallvec![]))
}

pub fn make_native_print_stack_trace(gas_params: PrintStackTraceGasParameters) -> NativeFunction {
    Arc::new(
        move |context, ty_args, args| -> PartialVMResult<NativeResult> {
            native_print_stack_trace(&gas_params, context, ty_args, args)
        },
    )
}

/***************************************************************************************************
 * module
 **************************************************************************************************/
#[derive(Debug, Clone)]
pub struct GasParameters {
    pub print: PrintGasParameters,
    pub print_stack_trace: PrintStackTraceGasParameters,
}

pub fn make_all(
    gas_params: GasParameters,
    move_std_addr: AccountAddress,
) -> impl Iterator<Item = (String, NativeFunction)> {
    let natives = [
        ("print", make_native_print(gas_params.print, move_std_addr)),
        (
            "print_stack_trace",
            make_native_print_stack_trace(gas_params.print_stack_trace),
        ),
    ];

    make_module_natives(natives)
}

#[cfg(feature = "testing")]
mod testing {
    use move_binary_format::errors::{PartialVMError, PartialVMResult};
    use move_core_types::{
        account_address::AccountAddress,
        language_storage::TypeTag,
        value::{MoveStruct, MoveStructLayout, MoveTypeLayout, MoveValue},
        vm_status::StatusCode,
    };
    use move_vm_runtime::native_functions::NativeContext;
    use move_vm_types::{loaded_data::runtime_types::Type, values::Value};
    use std::{fmt, fmt::Write};

    const VECTOR_BEGIN: &str = "[";

    const VECTOR_OR_STRUCT_SEP: &str = ",";

    const VECTOR_END: &str = "]";

    const STRUCT_BEGIN: &str = "{";

    const STRUCT_END: &str = "}";

    fn fmt_error_to_partial_vm_error(e: fmt::Error) -> PartialVMError {
        PartialVMError::new(StatusCode::UNKNOWN_STATUS)
            .with_message("write! macro failed with: ".to_string() + e.to_string().as_str())
    }

    fn to_vec_u8_type_err<E>(_e: E) -> PartialVMError {
        PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
            .with_message("Could not convert Vec<MoveValue> to Vec<u8>: ".to_string())
    }

    fn get_annotated_struct_layout(
        context: &NativeContext,
        ty: &Type,
    ) -> PartialVMResult<MoveStructLayout> {
        let annotated_type_layout = context.type_to_fully_annotated_layout(ty)?.unwrap();
        match annotated_type_layout {
            MoveTypeLayout::Struct(annotated_struct_layout) => Ok(annotated_struct_layout),
            _ => Err(
                PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR).with_message(
                    "Could not convert Type to fully-annotated MoveTypeLayout via NativeContext"
                        .to_string(),
                ),
            ),
        }
    }

    fn get_vector_inner_type(ty: &Type) -> PartialVMResult<&Type> {
        match ty {
            Type::Vector(ty) => Ok(ty),
            _ => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                .with_message("Could not get the inner Type of a vector's Type".to_string())),
        }
    }

    /// Converts a `MoveValue::Vector` of `u8`'s to a `String` by wrapping it in double quotes and
    /// escaping double quotes and backslashes.
    ///
    /// Examples:
    ///  - 'Hello' returns "Hello"
    ///  - '"Hello?" What are you saying?' returns "\"Hello?\" What are you saying?"
    ///  - '\ and " are escaped' returns "\\ and \" are escaped"
    fn move_value_as_escaped_string(val: MoveValue) -> PartialVMResult<String> {
        match val {
            MoveValue::Vector(bytes) => {
                let buf = MoveValue::vec_to_vec_u8(bytes).map_err(to_vec_u8_type_err)?;

                let str = String::from_utf8(buf).map_err(|e| {
                    PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR).with_message(
                        "Could not parse UTF8 bytes: ".to_string() + e.to_string().as_str(),
                    )
                })?;

                // We need to escape displayed double quotes " as \" and, as a result, also escape
                // displayed \ as \\.
                Ok(str.replace('\\', "\\\\").replace('"', "\\\""))
            }
            _ => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                .with_message("Expected a MoveValue::Vector of u8's".to_string())),
        }
    }

    fn print_padding_at_depth(out: &mut String, depth: usize) -> PartialVMResult<()> {
        for _ in 0..depth {
            // add 2 spaces
            write!(out, "  ").map_err(fmt_error_to_partial_vm_error)?;
        }

        Ok(())
    }

    fn is_non_empty_vector_u8(vec: &Vec<MoveValue>) -> bool {
        if vec.is_empty() {
            false
        } else {
            matches!(vec.last().unwrap(), MoveValue::U8(_))
        }
    }

    fn is_vector_or_struct_move_value(mv: &MoveValue) -> bool {
        matches!(mv, MoveValue::Vector(_) | MoveValue::Struct(_))
    }

    /// Prints any `Value` in a user-friendly manner.
    pub(crate) fn print_value(
        context: &NativeContext,
        out: &mut String,
        val: Value,
        ty: Type,
        move_std_addr: &AccountAddress,
        depth: usize,
        canonicalize: bool,
        single_line: bool,
        include_int_types: bool,
    ) -> PartialVMResult<()> {
        // get type layout in VM format
        let ty_layout = context.type_to_type_layout(&ty)?.unwrap();

        match &ty_layout {
            MoveTypeLayout::Vector(_) => {
                // get the inner type T of a vector<T>
                let inner_ty = get_vector_inner_type(&ty)?;
                let inner_tyl = context.type_to_type_layout(inner_ty)?.unwrap();

                match inner_tyl {
                    // We cannot simply convert a `Value` (of type vector) to a `MoveValue` because
                    // there might be a struct in the vector that needs to be "decorated" using the
                    // logic in this function. Instead, we recursively "unpack" the vector until we
                    // get down to either (1) a primitive type, which we can forward to
                    // `print_move_value`, or (2) a struct type, which we can decorate and forward
                    // to `print_move_value`.
                    MoveTypeLayout::Vector(_) | MoveTypeLayout::Struct(_) => {
                        // `val` is either a `Vec<Vec<Value>>`, a `Vec<Struct>`,  or a `Vec<signer>`, so we cast `val` as a `Vec<Value>` and call ourselves recursively
                        let vec = val.value_as::<Vec<Value>>()?;

                        let print_inner_value =
                            |out: &mut String,
                             val: Value,
                             move_std_addr: &AccountAddress,
                             depth: usize,
                             canonicalize: bool,
                             single_line: bool,
                             include_int_types: bool| {
                                print_value(
                                    context,
                                    out,
                                    val,
                                    inner_ty.clone(),
                                    move_std_addr,
                                    depth,
                                    canonicalize,
                                    single_line,
                                    include_int_types,
                                )
                            };

                        print_non_u8_vector(
                            out,
                            move_std_addr,
                            depth,
                            canonicalize,
                            single_line,
                            include_int_types,
                            vec,
                            print_inner_value,
                            true,
                        )?;
                    }
                    // If the inner type T of this vector<T> is a primitive bool/unsigned integer/address type, we convert the
                    // vector<T> to a MoveValue and print it.
                    _ => {
                        let mv = val.as_move_value(&ty_layout);
                        print_move_value(
                            out,
                            mv,
                            move_std_addr,
                            depth,
                            canonicalize,
                            single_line,
                            include_int_types,
                        )?;
                    }
                };
            }
            // For a struct, we convert it to a MoveValue annotated with its field names and types and print it
            MoveTypeLayout::Struct(_) => {
                let move_struct = match val.as_move_value(&ty_layout) {
                    MoveValue::Struct(s) => s,
                    _ => {
                        return Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                            .with_message("Expected MoveValue::MoveStruct".to_string()))
                    }
                };

                let annotated_struct_layout = get_annotated_struct_layout(context, &ty)?;
                let decorated_struct = move_struct.decorate(&annotated_struct_layout);

                print_move_value(
                    out,
                    MoveValue::Struct(decorated_struct),
                    move_std_addr,
                    depth,
                    canonicalize,
                    single_line,
                    include_int_types,
                )?;
            }
            // For non-structs and non-vectors, convert them to a MoveValue and print them
            _ => {
                print_move_value(
                    out,
                    val.as_move_value(&ty_layout),
                    move_std_addr,
                    depth,
                    canonicalize,
                    single_line,
                    include_int_types,
                )?;
            }
        }

        Ok(())
    }

    /// Prints the MoveValue in `mv`, optionally-printing integer types if `include_int_type` is
    /// true.
    fn print_move_value(
        out: &mut String,
        mv: MoveValue,
        move_std_addr: &AccountAddress,
        depth: usize,
        canonicalize: bool,
        single_line: bool,
        include_int_types: bool,
    ) -> PartialVMResult<()> {
        match mv {
            MoveValue::U8(u8) => {
                write!(out, "{}", u8).map_err(fmt_error_to_partial_vm_error)?;
                if include_int_types {
                    write!(out, "u8").map_err(fmt_error_to_partial_vm_error)?;
                }
            }
            MoveValue::U16(u16) => {
                write!(out, "{}", u16).map_err(fmt_error_to_partial_vm_error)?;
                if include_int_types {
                    write!(out, "u16").map_err(fmt_error_to_partial_vm_error)?;
                }
            }
            MoveValue::U32(u32) => {
                write!(out, "{}", u32).map_err(fmt_error_to_partial_vm_error)?;
                if include_int_types {
                    write!(out, "u32").map_err(fmt_error_to_partial_vm_error)?;
                }
            }
            MoveValue::U64(u64) => {
                write!(out, "{}", u64).map_err(fmt_error_to_partial_vm_error)?;
                if include_int_types {
                    write!(out, "u64").map_err(fmt_error_to_partial_vm_error)?;
                }
            }
            MoveValue::U128(u128) => {
                write!(out, "{}", u128).map_err(fmt_error_to_partial_vm_error)?;
                if include_int_types {
                    write!(out, "u128").map_err(fmt_error_to_partial_vm_error)?;
                }
            }
            MoveValue::U256(u256) => {
                write!(out, "{}", u256).map_err(fmt_error_to_partial_vm_error)?;
                if include_int_types {
                    write!(out, "u256").map_err(fmt_error_to_partial_vm_error)?;
                }
            }
            MoveValue::Bool(b) => {
                // Note that when `include_int_types` is enabled, the boolean `true` and `false`
                // values unambiguously encode their type, since they are different than any integer
                // type value, address value, signer value, vector value and struct value.
                write!(out, "{}", if b { "true" } else { "false" })
                    .map_err(fmt_error_to_partial_vm_error)?;
            }
            MoveValue::Address(a) => {
                let str = if canonicalize {
                    a.to_canonical_string()
                } else {
                    a.to_hex_literal()
                };
                write!(out, "@{}", str).map_err(fmt_error_to_partial_vm_error)?;
            }
            MoveValue::Signer(s) => {
                let str = if canonicalize {
                    s.to_canonical_string()
                } else {
                    s.to_hex_literal()
                };
                write!(out, "signer({})", str).map_err(fmt_error_to_partial_vm_error)?;
            }
            MoveValue::Vector(vec) => {
                // If this is a vector<u8> we print it in hex (as most users would expect us to)
                if is_non_empty_vector_u8(&vec) {
                    let bytes = MoveValue::vec_to_vec_u8(vec).map_err(to_vec_u8_type_err)?;
                    write!(out, "0x{}", hex::encode(&bytes))
                        .map_err(fmt_error_to_partial_vm_error)?;
                } else {
                    let is_complex_inner_type =
                        vec.last().map_or(false, is_vector_or_struct_move_value);
                    print_non_u8_vector(
                        out,
                        move_std_addr,
                        depth,
                        canonicalize,
                        single_line,
                        include_int_types,
                        vec,
                        print_move_value,
                        is_complex_inner_type,
                    )?;
                }
            }
            MoveValue::Struct(move_struct) => match move_struct {
                MoveStruct::WithTypes { type_, mut fields } => {
                    let type_tag = TypeTag::from(type_.clone());

                    // Check if struct is an std::string::String
                    if !canonicalize && type_.is_std_string(move_std_addr) {
                        if fields.len() != 1 {
                            return Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                                .with_message(
                                    "Expected std::string::String struct to have just one field"
                                        .to_string(),
                                ));
                        }

                        let (id, val) = fields.pop().unwrap();
                        if id.into_string() != "bytes" {
                            return Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                                .with_message(
                                    "Expected std::string::String struct to have a `bytes` field"
                                        .to_string(),
                                ));
                        }

                        let str = move_value_as_escaped_string(val)?;
                        write!(out, "\"{}\"", str).map_err(fmt_error_to_partial_vm_error)?
                    } else if !canonicalize && type_.is_ascii_string(move_std_addr) {
                        if fields.len() != 1 {
                            return Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                                .with_message(
                                    "Expected std::ascii::String struct to have just one field"
                                        .to_string(),
                                ));
                        }

                        let (id, val) = fields.pop().unwrap();
                        if id.into_string() != "bytes" {
                            return Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                                .with_message(
                                    "Expected std::ascii::String struct to have a `bytes` field"
                                        .to_string(),
                                ));
                        }

                        let str = move_value_as_escaped_string(val)?;
                        write!(out, "\"{}\"", str).map_err(fmt_error_to_partial_vm_error)?
                    } else {
                        write!(out, "{} ", type_tag).map_err(fmt_error_to_partial_vm_error)?;
                        write!(out, "{}", STRUCT_BEGIN).map_err(fmt_error_to_partial_vm_error)?;

                        // For each field, print its name and value (and type)
                        let mut iter = fields.into_iter();
                        if let Some((field_name, field_value)) = iter.next() {
                            // Start an indented new line
                            if !single_line {
                                writeln!(out).map_err(fmt_error_to_partial_vm_error)?;
                                print_padding_at_depth(out, depth + 1)?;
                            }

                            write!(out, "{}: ", field_name.into_string())
                                .map_err(fmt_error_to_partial_vm_error)?;
                            print_move_value(
                                out,
                                field_value,
                                move_std_addr,
                                depth + 1,
                                canonicalize,
                                single_line,
                                include_int_types,
                            )?;

                            for (field_name, field_value) in iter {
                                write!(out, "{}", VECTOR_OR_STRUCT_SEP)
                                    .map_err(fmt_error_to_partial_vm_error)?;

                                if !single_line {
                                    writeln!(out).map_err(fmt_error_to_partial_vm_error)?;
                                    print_padding_at_depth(out, depth + 1)?;
                                } else {
                                    write!(out, " ").map_err(fmt_error_to_partial_vm_error)?;
                                }
                                write!(out, "{}: ", field_name.into_string())
                                    .map_err(fmt_error_to_partial_vm_error)?;
                                print_move_value(
                                    out,
                                    field_value,
                                    move_std_addr,
                                    depth + 1,
                                    canonicalize,
                                    single_line,
                                    include_int_types,
                                )?;
                            }
                        }

                        // Ends printing the struct with "}", which could be on its own line
                        if !single_line {
                            writeln!(out).map_err(fmt_error_to_partial_vm_error)?;
                            print_padding_at_depth(out, depth)?;
                        }
                        write!(out, "{}", STRUCT_END).map_err(fmt_error_to_partial_vm_error)?;
                    }
                }
                _ => {
                    return Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                        .with_message("Expected MoveStruct::WithTypes".to_string()))
                }
            },
        }

        Ok(())
    }

    fn print_non_u8_vector<ValType>(
        out: &mut String,
        move_std_addr: &AccountAddress,
        depth: usize,
        canonicalize: bool,
        single_line: bool,
        include_int_types: bool,
        vec: Vec<ValType>,
        print_inner_value: impl Fn(
            &mut String,
            ValType,
            &AccountAddress,
            usize,
            bool,
            bool,
            bool,
        ) -> PartialVMResult<()>,
        is_complex_inner_type: bool,
    ) -> PartialVMResult<()> {
        write!(out, "{}", VECTOR_BEGIN).map_err(fmt_error_to_partial_vm_error)?;
        let mut iter = vec.into_iter();
        let mut empty_vec = true;

        if let Some(first_elem) = iter.next() {
            empty_vec = false;

            // For vectors-of-vectors, and for vectors-of-structs, we start a newline for each element
            if !single_line && is_complex_inner_type {
                writeln!(out).map_err(fmt_error_to_partial_vm_error)?;
                print_padding_at_depth(out, depth + 1)?;
            } else {
                write!(out, " ").map_err(fmt_error_to_partial_vm_error)?;
            }

            print_inner_value(
                out,
                first_elem,
                move_std_addr,
                depth + 1,
                canonicalize,
                single_line,
                include_int_types,
            )?;

            for elem in iter {
                write!(out, "{}", VECTOR_OR_STRUCT_SEP).map_err(fmt_error_to_partial_vm_error)?;

                // For vectors of vectors or vectors of structs, we start a newline for each element
                if !single_line && is_complex_inner_type {
                    writeln!(out).map_err(fmt_error_to_partial_vm_error)?;
                    print_padding_at_depth(out, depth + 1)?;
                } else {
                    write!(out, " ").map_err(fmt_error_to_partial_vm_error)?;
                }
                print_inner_value(
                    out,
                    elem,
                    move_std_addr,
                    depth + 1,
                    canonicalize,
                    single_line,
                    include_int_types,
                )?;
            }
        }

        // For vectors of vectors or vectors of structs, we display the closing ] on a newline
        if !single_line && is_complex_inner_type {
            writeln!(out).map_err(fmt_error_to_partial_vm_error)?;
            print_padding_at_depth(out, depth)?;
        } else if !empty_vec {
            write!(out, " ").map_err(fmt_error_to_partial_vm_error)?;
        }
        write!(out, "{}", VECTOR_END).map_err(fmt_error_to_partial_vm_error)?;

        Ok(())
    }
}
