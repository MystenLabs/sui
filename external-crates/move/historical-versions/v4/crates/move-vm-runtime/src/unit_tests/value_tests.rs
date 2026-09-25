// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Write as _;

use crate::{
    execution::{
        interpreter::locals::MachineHeap,
        values::{debug::print_value, *},
    },
    jit::execution::ast::Type,
    shared::views::*,
};
use move_binary_format::{errors::*, file_format::VariantTag};
use move_core_types::{account_address::AccountAddress, runtime_value, u256::U256};

#[cfg(test)]
const SIZE_CONFIG: SizeConfig = SizeConfig {
    traverse_references: false,
    include_vector_size: true,
};

#[cfg(test)]
const SIZE_CONFIG_TRAVERSE: SizeConfig = SizeConfig {
    traverse_references: true,
    include_vector_size: true,
};

#[test]
fn locals() -> PartialVMResult<()> {
    const LEN: usize = 4;
    let mut heap = MachineHeap::new();
    let mut locals = heap.allocate_stack_frame(vec![], LEN)?;

    for i in 0..LEN {
        assert!(locals.copy_loc(i).is_err());
        assert!(locals.move_loc(i).is_err());
        assert!(locals.borrow_loc(i).is_err());
    }
    locals.store_loc(1, Value::u64(42))?;

    assert!(locals.copy_loc(1)?.equals(&Value::u64(42))?);
    let r: Reference = VMValueCast::cast(locals.borrow_loc(1)?)?;
    assert!(r.read_ref()?.equals(&Value::u64(42))?);
    assert!(locals.move_loc(1)?.equals(&Value::u64(42))?);

    assert!(locals.copy_loc(1).is_err());
    assert!(locals.move_loc(1).is_err());
    assert!(locals.borrow_loc(1).is_err());

    assert!(locals.copy_loc(LEN + 1).is_err());
    assert!(locals.move_loc(LEN + 1).is_err());
    assert!(locals.borrow_loc(LEN + 1).is_err());

    Ok(())
}

#[test]
fn struct_pack_and_unpack() -> PartialVMResult<()> {
    let vals = [
        Value::u8(10),
        Value::u16(12),
        Value::u32(15),
        Value::u64(20),
        Value::u128(30),
        Value::u256(U256::max_value()),
    ];
    let s = Struct::pack([
        Value::u8(10),
        Value::u16(12),
        Value::u32(15),
        Value::u64(20),
        Value::u128(30),
        Value::u256(U256::max_value()),
    ]);
    let unpacked: Vec<_> = s.unpack().collect();

    assert!(vals.len() == unpacked.len());
    for (v1, v2) in vals.iter().zip(unpacked.iter()) {
        assert!(v1.equals(v2)?);
    }

    Ok(())
}

#[test]
fn struct_borrow_field() -> PartialVMResult<()> {
    let mut heap = MachineHeap::new();
    let mut locals = heap.allocate_stack_frame(vec![], 1)?;

    locals.store_loc(
        0,
        Value::make_struct(vec![Value::u8(10), Value::bool(false)]),
    )?;
    let r: StructRef = VMValueCast::cast(locals.borrow_loc(0)?)?;

    {
        let f: Reference = VMValueCast::cast(r.borrow_field(1)?)?;
        assert!(f.read_ref()?.equals(&Value::bool(false))?);
    }

    {
        let f: Reference = VMValueCast::cast(r.borrow_field(1)?)?;
        f.write_ref(Value::bool(true))?;
    }

    {
        let f: Reference = VMValueCast::cast(r.borrow_field(1)?)?;
        assert!(f.read_ref()?.equals(&Value::bool(true))?);
    }

    Ok(())
}

#[test]
fn struct_borrow_nested() -> PartialVMResult<()> {
    let mut heap = MachineHeap::new();
    let mut locals = heap.allocate_stack_frame(vec![], 1)?;

    fn inner(x: u64) -> Value {
        Value::make_struct(vec![Value::u64(x)])
    }
    fn outer(x: u64) -> Value {
        Value::make_struct(vec![Value::u8(10), inner(x)])
    }

    locals.store_loc(0, outer(20))?;
    let r1: StructRef = VMValueCast::cast(locals.borrow_loc(0)?)?;
    let r2: StructRef = VMValueCast::cast(r1.borrow_field(1)?)?;

    {
        let r3: Reference = VMValueCast::cast(r2.borrow_field(0)?)?;
        assert!(r3.read_ref()?.equals(&Value::u64(20))?);
    }

    {
        let r3: Reference = VMValueCast::cast(r2.borrow_field(0)?)?;
        r3.write_ref(Value::u64(30))?;
    }

    {
        let r3: Reference = VMValueCast::cast(r2.borrow_field(0)?)?;
        assert!(r3.read_ref()?.equals(&Value::u64(30))?);
    }

    assert!(r2.read_ref()?.equals(&inner(30))?);
    assert!(r1.read_ref()?.equals(&outer(30))?);

    Ok(())
}

#[test]
fn vec_and_ref_eq() -> PartialVMResult<()> {
    let v = MemBox::new(Value::PrimVec(PrimVec::VecU8(vec![10, 12])));
    let x = MemBox::new(Value::u8(12));
    let v_ref: VectorRef = VMValueCast::cast(v.as_ref_value())?;
    let v_1_ref = v_ref.borrow_elem(1, &Type::U8)?;
    let x_ref = x.as_ref_value();
    assert!(v_1_ref.equals(&x_ref)?);
    let v_0_ref = v_ref.borrow_elem(0, &Type::U8)?;
    assert!(!v_0_ref.equals(&x_ref)?);
    Ok(())
}

#[test]
fn global_value_non_struct() -> PartialVMResult<()> {
    assert!(
        GlobalValue::create(Value::u64(100)).is_err(),
        "cache error 0"
    );
    assert!(
        GlobalValue::create(Value::bool(false)).is_err(),
        "cache error 1"
    );

    let mut heap = MachineHeap::new();
    let mut locals = heap.allocate_stack_frame(vec![], 1)?;

    locals.store_loc(0, Value::u8(0)).expect("stored");
    let r = locals.borrow_loc(0).expect("borrowed");
    assert!(GlobalValue::create(r).is_err(), "cache error 2");

    let _ = heap.free_stack_frame(locals);

    Ok(())
}

fn print_val(v: &Value) -> String {
    let mut s = String::new();
    print_value(&mut s, v).unwrap();
    s
}

fn print_ref(r: &Reference) -> String {
    format!("(&) {}", print_val(&r.copy_value().read_ref().unwrap()))
}

#[test]
fn ref_abstract_memory_size_consistency() -> PartialVMResult<()> {
    let mut heap = MachineHeap::new();
    let mut locals = heap.allocate_stack_frame(vec![], 10)?;

    let mut output = String::new();

    let mut record_val_size = |val: &Value| {
        let val_size = val.abstract_memory_size(&SIZE_CONFIG).unwrap();
        writeln!(&mut output, "size of {:?}: {}, ", print_val(val), val_size).unwrap();
    };

    locals.store_loc(0, Value::u128(0))?;
    let r = locals.borrow_loc(0)?;
    record_val_size(&r);

    locals.store_loc(1, Value::vector_u8([1, 2, 3]))?;
    let r = locals.borrow_loc(1)?;
    record_val_size(&r);

    let r: VectorRef = VMValueCast::cast(r)?;
    let r = r.borrow_elem(0, &Type::U8)?;
    record_val_size(&r);

    locals.store_loc(2, Value::make_struct(vec![]))?;
    let r: Reference = VMValueCast::cast(locals.borrow_loc(2)?)?;
    let val_size = r.abstract_memory_size(&SIZE_CONFIG).unwrap();
    writeln!(&mut output, "size of {:?}: {}, ", print_ref(&r), val_size).unwrap();

    let val_size_traverse = r.abstract_memory_size(&SIZE_CONFIG_TRAVERSE).unwrap();
    writeln!(
        &mut output,
        "traversed size of {:?}: {}, ",
        print_ref(&r),
        val_size_traverse
    )
    .unwrap();

    insta::assert_snapshot!(output);

    Ok(())
}

#[test]
fn struct_abstract_memory_size_consistenty() -> PartialVMResult<()> {
    let structs = [
        Struct::pack([]),
        Struct::pack([Value::make_struct(vec![Value::u8(0), Value::u64(0)])]),
    ];

    let mut output = String::new();

    for s in structs {
        writeln!(
            &mut output,
            "size of struct {1:?}: {0}",
            s.abstract_memory_size(&SIZE_CONFIG).unwrap(),
            print_val(&Value::Struct(s)),
        )
        .unwrap();
    }
    insta::assert_snapshot!(output);

    Ok(())
}

#[test]
fn val_abstract_memory_size_consistency() -> PartialVMResult<()> {
    let mut output = String::new();
    let vals = [
        Value::u8(0),
        Value::u16(0),
        Value::u32(0),
        Value::u64(0),
        Value::u128(0),
        Value::u256(U256::zero()),
        Value::bool(true),
        Value::address(AccountAddress::ZERO),
        Value::vector_u8([0, 1, 2]),
        Value::vector_u16([0, 1, 2]),
        Value::vector_u32([0, 1, 2]),
        Value::vector_u64([]),
        Value::vector_u128([1, 2, 3, 4]),
        Value::vector_u256([1u64, 2, 3, 4].iter().map(|q| U256::from(*q))),
        Value::make_struct([]),
        Value::make_struct([Value::u8(0), Value::bool(false)]),
        Vector::pack(VectorSpecialization::Container, [])?,
        Vector::pack(VectorSpecialization::U8, [Value::u8(0), Value::u8(1)])?,
    ];

    let mut heap = MachineHeap::new();
    let mut locals = heap.allocate_stack_frame(vec![], vals.len())?;

    let record_val_size = |output: &mut String, val: &Value| {
        let val_size = val.abstract_memory_size(&SIZE_CONFIG).unwrap();
        writeln!(output, "size of {:?}: {}, ", print_val(val), val_size).unwrap();
    };

    let record_ref_size = |output: &mut String, val: &Reference| {
        let val_size = val.abstract_memory_size(&SIZE_CONFIG).unwrap();
        let val_size_traverse = val.abstract_memory_size(&SIZE_CONFIG_TRAVERSE).unwrap();
        writeln!(output, "size of {:?}: {}, ", print_ref(val), val_size).unwrap();
        writeln!(
            output,
            "traversed size of {:?}: {}, ",
            print_ref(val),
            val_size_traverse
        )
        .unwrap();
    };

    for (idx, val) in vals.iter().enumerate() {
        locals.store_loc(idx, val.copy_value())?;

        record_val_size(&mut output, val);

        let ref_: Reference = VMValueCast::cast(locals.borrow_loc(idx)?)?;
        record_ref_size(&mut output, &ref_);
    }

    insta::assert_snapshot!(output);

    Ok(())
}

#[test]
fn test_vm_value_vector_u64_casting() {
    assert_eq!(
        vec![1, 2, 3],
        VMValueCast::<Vec<u64>>::cast(Value::vector_u64([1, 2, 3])).unwrap()
    );
}

#[test]
fn assert_sizes() {
    assert_eq!(size_of::<Value>(), 32);
}

#[test]
fn signer_equivalence() -> PartialVMResult<()> {
    let addr = AccountAddress::TWO;
    let signer = Value::signer(addr);

    assert_eq!(
        signer.serialize(),
        signer.typed_serialize(&runtime_value::MoveTypeLayout::Signer)
    );

    assert_eq!(
        signer.serialize(),
        signer.typed_serialize(&runtime_value::MoveTypeLayout::Struct(Box::new(
            runtime_value::MoveStructLayout(Box::new(vec![runtime_value::MoveTypeLayout::Address]))
        )))
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Vector Operations — VectorRef push_back/pop/swap/len/borrow_elem on
// PrimVec and Container (generic Vec) specializations.
// ---------------------------------------------------------------------------

/// push_back grows a PrimVec and the new element is readable via borrow_elem.
#[test]
fn vector_push_back_and_len_prim_vec() -> PartialVMResult<()> {
    let v = MemBox::new(Value::vector_u8([1, 2, 3]));
    let vr: VectorRef = VMValueCast::cast(v.as_ref_value())?;

    assert!(vr.len(&Type::U8)?.equals(&Value::u64(3))?);

    vr.push_back(Value::u8(4), &Type::U8, 100)?;
    assert!(vr.len(&Type::U8)?.equals(&Value::u64(4))?);

    let elem = vr.borrow_elem(3, &Type::U8)?;
    let r: Reference = VMValueCast::cast(elem)?;
    assert!(r.read_ref()?.equals(&Value::u8(4))?);

    Ok(())
}

/// pop returns the last element and shrinks the vector by one.
#[test]
fn vector_pop_prim_vec() -> PartialVMResult<()> {
    let v = MemBox::new(Value::vector_u64([10, 20, 30]));
    let vr: VectorRef = VMValueCast::cast(v.as_ref_value())?;

    let popped = vr.pop(&Type::U64)?;
    assert!(popped.equals(&Value::u64(30))?);
    assert!(vr.len(&Type::U64)?.equals(&Value::u64(2))?);

    Ok(())
}

/// Popping an empty vector produces an error.
#[test]
fn vector_pop_empty_errors() -> PartialVMResult<()> {
    let v = MemBox::new(Value::vector_u8([]));
    let vr: VectorRef = VMValueCast::cast(v.as_ref_value())?;

    assert!(vr.pop(&Type::U8).is_err());
    Ok(())
}

/// swap exchanges two elements in a PrimVec.
#[test]
fn vector_swap_prim_vec() -> PartialVMResult<()> {
    let v = MemBox::new(Value::vector_u64([10, 20, 30]));
    let vr: VectorRef = VMValueCast::cast(v.as_ref_value())?;

    vr.swap(0, 2, &Type::U64)?;

    let r0: Reference = VMValueCast::cast(vr.borrow_elem(0, &Type::U64)?)?;
    assert!(r0.read_ref()?.equals(&Value::u64(30))?);
    let r2: Reference = VMValueCast::cast(vr.borrow_elem(2, &Type::U64)?)?;
    assert!(r2.read_ref()?.equals(&Value::u64(10))?);

    Ok(())
}

/// swap with an out-of-bounds index errors.
#[test]
fn vector_swap_out_of_bounds() -> PartialVMResult<()> {
    let v = MemBox::new(Value::vector_u8([1, 2]));
    let vr: VectorRef = VMValueCast::cast(v.as_ref_value())?;

    assert!(vr.swap(0, 5, &Type::U8).is_err());
    Ok(())
}

/// borrow_elem with an out-of-bounds index errors.
#[test]
fn vector_borrow_elem_out_of_bounds() -> PartialVMResult<()> {
    let v = MemBox::new(Value::vector_u8([1]));
    let vr: VectorRef = VMValueCast::cast(v.as_ref_value())?;

    assert!(vr.borrow_elem(10, &Type::U8).is_err());
    Ok(())
}

/// push_back/pop on the Container (non-primitive) specialization.
#[test]
fn vector_push_back_and_pop_container_vec() -> PartialVMResult<()> {
    let inner1 = Value::vector_u8([1, 2, 3]);
    let inner2 = Value::vector_u8([4, 5]);
    let vec_val = Vector::pack(VectorSpecialization::Container, [inner1.copy_value()])?;
    let v = MemBox::new(vec_val);
    let vr: VectorRef = VMValueCast::cast(v.as_ref_value())?;

    // Vector-of-vectors maps to Container specialization.
    let ty = Type::Vector(Box::new(Type::U8));
    vr.push_back(inner2.copy_value(), &ty, 100)?;
    assert!(vr.len(&ty)?.equals(&Value::u64(2))?);

    let popped = vr.pop(&ty)?;
    assert!(popped.equals(&inner2)?);

    Ok(())
}

/// Vector::pack followed by unpack preserves values for PrimVec (U64).
#[test]
fn vector_pack_unpack_prim() -> PartialVMResult<()> {
    let packed = Vector::pack(VectorSpecialization::U64, [Value::u64(5), Value::u64(10)])?;
    let vec: Vector = VMValueCast::cast(packed)?;
    let unpacked = vec.unpack(&Type::U64, 2)?;

    assert_eq!(unpacked.len(), 2);
    assert!(unpacked[0].equals(&Value::u64(5))?);
    assert!(unpacked[1].equals(&Value::u64(10))?);
    Ok(())
}

/// unpack with a mismatched expected_num errors.
#[test]
fn vector_unpack_wrong_count_errors() -> PartialVMResult<()> {
    let packed = Vector::pack(VectorSpecialization::U8, [Value::u8(1), Value::u8(2)])?;
    let vec: Vector = VMValueCast::cast(packed)?;
    assert!(vec.unpack(&Type::U8, 5).is_err());
    Ok(())
}

// ---------------------------------------------------------------------------
// Variant Operations — Variant pack/unpack/check_tag and VariantRef
// get_tag/check_tag/unpack_variant on borrowed variants.
// ---------------------------------------------------------------------------

/// Variant::pack -> unpack round-trip preserves tag and field values.
#[test]
fn variant_pack_unpack_round_trip() -> PartialVMResult<()> {
    let tag: VariantTag = 1;
    let fields = [Value::u64(42), Value::bool(true)];
    let variant = Variant::pack(tag, fields.iter().map(Value::copy_value));

    assert_eq!(variant.len(), 2);
    variant.check_tag(1)?;

    let unpacked: Vec<_> = variant.unpack().collect();
    assert_eq!(unpacked.len(), 2);
    assert!(unpacked[0].equals(&Value::u64(42))?);
    assert!(unpacked[1].equals(&Value::bool(true))?);
    Ok(())
}

/// check_tag succeeds on match, errors on mismatch.
#[test]
fn variant_check_tag_success_and_failure() -> PartialVMResult<()> {
    let variant = Variant::pack(2, vec![Value::u8(10)]);
    assert!(variant.check_tag(2).is_ok());
    assert!(variant.check_tag(0).is_err());
    Ok(())
}

/// VariantRef::get_tag reads the tag and unpack_variant yields field references.
#[test]
fn variant_ref_get_tag_and_unpack() -> PartialVMResult<()> {
    let mut heap = MachineHeap::new();
    let mut locals = heap.allocate_stack_frame(vec![], 1)?;

    locals.store_loc(0, Value::make_variant(1, vec![Value::u64(99)]))?;
    let vr: VariantRef = VMValueCast::cast(locals.borrow_loc(0)?)?;

    assert_eq!(vr.get_tag()?, 1);

    let field_refs = vr.unpack_variant()?;
    assert_eq!(field_refs.len(), 1);
    let r: Reference = VMValueCast::cast(field_refs.into_iter().next().unwrap())?;
    assert!(r.read_ref()?.equals(&Value::u64(99))?);

    Ok(())
}

/// VariantRef::check_tag errors when the tag doesn't match.
#[test]
fn variant_ref_check_tag_mismatch() -> PartialVMResult<()> {
    let mut heap = MachineHeap::new();
    let mut locals = heap.allocate_stack_frame(vec![], 1)?;

    locals.store_loc(0, Value::make_variant(0, vec![Value::bool(false)]))?;
    let vr: VariantRef = VMValueCast::cast(locals.borrow_loc(0)?)?;

    assert!(vr.check_tag(5).is_err());
    Ok(())
}

// ---------------------------------------------------------------------------
// GlobalValue Lifecycle — empty/move_to/borrow_global/move_from/into_value.
// (global_value_non_struct above covers the create-rejects-non-struct path.)
// ---------------------------------------------------------------------------

/// Full lifecycle: empty -> move_to -> borrow_global -> move_from -> empty.
#[test]
fn global_value_full_lifecycle() -> PartialVMResult<()> {
    let mut gv = GlobalValue::empty();
    assert!(!gv.exists()?);
    assert!(gv.borrow_global().is_err());

    let s = Value::make_struct(vec![Value::u64(100)]);
    gv.move_to(s).unwrap();
    assert!(gv.exists()?);
    assert!(gv.borrow_global().is_ok());

    // borrow_global returns a reference to the struct
    let sr: StructRef = VMValueCast::cast(gv.borrow_global()?)?;
    let f: Reference = VMValueCast::cast(sr.borrow_field(0)?)?;
    assert!(f.read_ref()?.equals(&Value::u64(100))?);

    // move_from extracts the value and empties the slot
    let moved = gv.move_from()?;
    assert!(!gv.exists()?);
    assert!(moved.equals(&Value::make_struct(vec![Value::u64(100)]))?);
    assert!(gv.borrow_global().is_err());

    Ok(())
}

/// move_to on an already-occupied slot errors
#[test]
fn global_value_move_to_occupied_errors() -> PartialVMResult<()> {
    let mut gv = GlobalValue::empty();
    gv.move_to(Value::make_struct(vec![Value::u8(1)])).unwrap();

    let result = gv.move_to(Value::make_struct(vec![Value::u8(2)]));
    assert!(result.is_err());
    Ok(())
}

/// into_value returns None for empty, Some(value) for filled.
#[test]
fn global_value_into_value() -> PartialVMResult<()> {
    let gv_empty = GlobalValue::empty();
    assert!(gv_empty.into_value()?.is_none());

    let gv_filled = GlobalValue::create(Value::make_struct(vec![Value::u64(42)]))?;
    let val = gv_filled.into_value()?;
    assert!(
        val.unwrap()
            .equals(&Value::make_struct(vec![Value::u64(42)]))?
    );
    Ok(())
}

/// Writes through a borrow_global ref are visible when the value is moved out.
#[test]
fn global_value_borrow_global_write_through() -> PartialVMResult<()> {
    let mut gv = GlobalValue::empty();
    gv.move_to(Value::make_struct(vec![Value::u64(1)])).unwrap();

    let borrow_ref: StructRef = VMValueCast::cast(gv.borrow_global()?)?;
    let field_ref: Reference = VMValueCast::cast(borrow_ref.borrow_field(0)?)?;
    field_ref.write_ref(Value::u64(999))?;

    let moved = gv.move_from()?;
    assert!(moved.equals(&Value::make_struct(vec![Value::u64(999)]))?);
    Ok(())
}

// ---------------------------------------------------------------------------
// Indexed References — write_ref through Reference::Indexed (the variant
// produced by VectorRef::borrow_elem on PrimVec elements).
// ---------------------------------------------------------------------------

/// Writing through an indexed ref updates the underlying PrimVec element.
#[test]
fn indexed_ref_write_ref_prim_vec() -> PartialVMResult<()> {
    let v = MemBox::new(Value::vector_u8([10, 20, 30]));
    let vr: VectorRef = VMValueCast::cast(v.as_ref_value())?;

    let elem_ref: Reference = VMValueCast::cast(vr.borrow_elem(1, &Type::U8)?)?;
    elem_ref.write_ref(Value::u8(99))?;

    let elem_ref2: Reference = VMValueCast::cast(vr.borrow_elem(1, &Type::U8)?)?;
    assert!(elem_ref2.read_ref()?.equals(&Value::u8(99))?);
    Ok(())
}

/// Writing the wrong type through an indexed ref errors (u64 into u8 vec).
#[test]
fn indexed_ref_write_ref_type_mismatch() -> PartialVMResult<()> {
    let v = MemBox::new(Value::vector_u8([10]));
    let vr: VectorRef = VMValueCast::cast(v.as_ref_value())?;
    let elem_ref: Reference = VMValueCast::cast(vr.borrow_elem(0, &Type::U8)?)?;

    assert!(elem_ref.write_ref(Value::u64(999)).is_err());
    Ok(())
}

// ---------------------------------------------------------------------------
// Deep Copy — copy_value produces an independent clone.
// ---------------------------------------------------------------------------

/// Mutating one copy of a struct doesn't affect the other.
#[test]
fn copy_value_deep_copy_semantics() -> PartialVMResult<()> {
    let original = Value::make_struct(vec![Value::u64(10), Value::bool(true)]);
    let copy = original.copy_value();

    assert!(original.equals(&copy)?);

    let mut heap = MachineHeap::new();
    let mut locals = heap.allocate_stack_frame(vec![], 2)?;
    locals.store_loc(0, original)?;
    locals.store_loc(1, copy)?;

    let sr: StructRef = VMValueCast::cast(locals.borrow_loc(0)?)?;
    let f: Reference = VMValueCast::cast(sr.borrow_field(0)?)?;
    f.write_ref(Value::u64(999))?;

    let v0 = locals.move_loc(0)?;
    let v1 = locals.move_loc(1)?;
    assert!(!v0.equals(&v1)?);
    assert!(v1.equals(&Value::make_struct(vec![Value::u64(10), Value::bool(true)]))?);
    Ok(())
}

// ---------------------------------------------------------------------------
// Integer Arithmetic Boundary Conditions — overflow/underflow at every width.
// ---------------------------------------------------------------------------

/// u8 arithmetic boundaries: MAX+1 overflows, 0-1 underflows, MAX*2 overflows.
#[test]
fn integer_arithmetic_boundary_u8() -> PartialVMResult<()> {
    // Addition overflow
    assert!(
        IntegerValue::U8(u8::MAX)
            .add_checked(IntegerValue::U8(1))
            .is_err()
    );
    // MAX + 0 succeeds
    assert_eq!(
        VMValueCast::<u8>::cast(
            IntegerValue::U8(u8::MAX)
                .add_checked(IntegerValue::U8(0))?
                .into_value()
        )?,
        u8::MAX
    );
    // Subtraction underflow
    assert!(
        IntegerValue::U8(0)
            .sub_checked(IntegerValue::U8(1))
            .is_err()
    );
    // MAX - MAX = 0
    assert_eq!(
        VMValueCast::<u8>::cast(
            IntegerValue::U8(u8::MAX)
                .sub_checked(IntegerValue::U8(u8::MAX))?
                .into_value()
        )?,
        0u8
    );
    // Multiplication overflow
    assert!(
        IntegerValue::U8(u8::MAX)
            .mul_checked(IntegerValue::U8(2))
            .is_err()
    );
    // MAX * 1 = MAX
    assert_eq!(
        VMValueCast::<u8>::cast(
            IntegerValue::U8(u8::MAX)
                .mul_checked(IntegerValue::U8(1))?
                .into_value()
        )?,
        u8::MAX
    );
    // Division: MAX / 1 = MAX, div by zero errors
    assert_eq!(
        VMValueCast::<u8>::cast(
            IntegerValue::U8(u8::MAX)
                .div_checked(IntegerValue::U8(1))?
                .into_value()
        )?,
        u8::MAX
    );
    assert!(
        IntegerValue::U8(1)
            .div_checked(IntegerValue::U8(0))
            .is_err()
    );
    // Remainder: MAX % MAX = 0, rem by zero errors
    assert_eq!(
        VMValueCast::<u8>::cast(
            IntegerValue::U8(u8::MAX)
                .rem_checked(IntegerValue::U8(u8::MAX))?
                .into_value()
        )?,
        0u8
    );
    assert!(
        IntegerValue::U8(1)
            .rem_checked(IntegerValue::U8(0))
            .is_err()
    );
    Ok(())
}

/// u16 arithmetic boundaries.
#[test]
fn integer_arithmetic_boundary_u16() -> PartialVMResult<()> {
    assert!(
        IntegerValue::U16(u16::MAX)
            .add_checked(IntegerValue::U16(1))
            .is_err()
    );
    assert_eq!(
        VMValueCast::<u16>::cast(
            IntegerValue::U16(u16::MAX)
                .add_checked(IntegerValue::U16(0))?
                .into_value()
        )?,
        u16::MAX
    );
    assert!(
        IntegerValue::U16(0)
            .sub_checked(IntegerValue::U16(1))
            .is_err()
    );
    assert_eq!(
        VMValueCast::<u16>::cast(
            IntegerValue::U16(u16::MAX)
                .sub_checked(IntegerValue::U16(u16::MAX))?
                .into_value()
        )?,
        0u16
    );
    assert!(
        IntegerValue::U16(u16::MAX)
            .mul_checked(IntegerValue::U16(2))
            .is_err()
    );
    assert_eq!(
        VMValueCast::<u16>::cast(
            IntegerValue::U16(u16::MAX)
                .mul_checked(IntegerValue::U16(1))?
                .into_value()
        )?,
        u16::MAX
    );
    assert!(
        IntegerValue::U16(1)
            .div_checked(IntegerValue::U16(0))
            .is_err()
    );
    assert!(
        IntegerValue::U16(1)
            .rem_checked(IntegerValue::U16(0))
            .is_err()
    );
    Ok(())
}

/// u32 arithmetic boundaries.
#[test]
fn integer_arithmetic_boundary_u32() -> PartialVMResult<()> {
    assert!(
        IntegerValue::U32(u32::MAX)
            .add_checked(IntegerValue::U32(1))
            .is_err()
    );
    assert_eq!(
        VMValueCast::<u32>::cast(
            IntegerValue::U32(u32::MAX)
                .add_checked(IntegerValue::U32(0))?
                .into_value()
        )?,
        u32::MAX
    );
    assert!(
        IntegerValue::U32(0)
            .sub_checked(IntegerValue::U32(1))
            .is_err()
    );
    assert_eq!(
        VMValueCast::<u32>::cast(
            IntegerValue::U32(u32::MAX)
                .sub_checked(IntegerValue::U32(u32::MAX))?
                .into_value()
        )?,
        0u32
    );
    assert!(
        IntegerValue::U32(u32::MAX)
            .mul_checked(IntegerValue::U32(2))
            .is_err()
    );
    assert_eq!(
        VMValueCast::<u32>::cast(
            IntegerValue::U32(u32::MAX)
                .mul_checked(IntegerValue::U32(1))?
                .into_value()
        )?,
        u32::MAX
    );
    assert!(
        IntegerValue::U32(1)
            .div_checked(IntegerValue::U32(0))
            .is_err()
    );
    assert!(
        IntegerValue::U32(1)
            .rem_checked(IntegerValue::U32(0))
            .is_err()
    );
    Ok(())
}

/// u64 arithmetic boundaries.
#[test]
fn integer_arithmetic_boundary_u64() -> PartialVMResult<()> {
    assert!(
        IntegerValue::U64(u64::MAX)
            .add_checked(IntegerValue::U64(1))
            .is_err()
    );
    assert_eq!(
        VMValueCast::<u64>::cast(
            IntegerValue::U64(u64::MAX)
                .add_checked(IntegerValue::U64(0))?
                .into_value()
        )?,
        u64::MAX
    );
    assert!(
        IntegerValue::U64(0)
            .sub_checked(IntegerValue::U64(1))
            .is_err()
    );
    assert_eq!(
        VMValueCast::<u64>::cast(
            IntegerValue::U64(u64::MAX)
                .sub_checked(IntegerValue::U64(u64::MAX))?
                .into_value()
        )?,
        0u64
    );
    assert!(
        IntegerValue::U64(u64::MAX)
            .mul_checked(IntegerValue::U64(2))
            .is_err()
    );
    assert_eq!(
        VMValueCast::<u64>::cast(
            IntegerValue::U64(u64::MAX)
                .mul_checked(IntegerValue::U64(1))?
                .into_value()
        )?,
        u64::MAX
    );
    assert!(
        IntegerValue::U64(1)
            .div_checked(IntegerValue::U64(0))
            .is_err()
    );
    assert!(
        IntegerValue::U64(1)
            .rem_checked(IntegerValue::U64(0))
            .is_err()
    );
    Ok(())
}

/// u128 arithmetic boundaries.
#[test]
fn integer_arithmetic_boundary_u128() -> PartialVMResult<()> {
    assert!(
        IntegerValue::U128(u128::MAX)
            .add_checked(IntegerValue::U128(1))
            .is_err()
    );
    assert_eq!(
        VMValueCast::<u128>::cast(
            IntegerValue::U128(u128::MAX)
                .add_checked(IntegerValue::U128(0))?
                .into_value()
        )?,
        u128::MAX
    );
    assert!(
        IntegerValue::U128(0)
            .sub_checked(IntegerValue::U128(1))
            .is_err()
    );
    assert_eq!(
        VMValueCast::<u128>::cast(
            IntegerValue::U128(u128::MAX)
                .sub_checked(IntegerValue::U128(u128::MAX))?
                .into_value()
        )?,
        0u128
    );
    assert!(
        IntegerValue::U128(u128::MAX)
            .mul_checked(IntegerValue::U128(2))
            .is_err()
    );
    assert_eq!(
        VMValueCast::<u128>::cast(
            IntegerValue::U128(u128::MAX)
                .mul_checked(IntegerValue::U128(1))?
                .into_value()
        )?,
        u128::MAX
    );
    assert!(
        IntegerValue::U128(1)
            .div_checked(IntegerValue::U128(0))
            .is_err()
    );
    assert!(
        IntegerValue::U128(1)
            .rem_checked(IntegerValue::U128(0))
            .is_err()
    );
    Ok(())
}

/// u256 arithmetic boundaries.
#[test]
fn integer_arithmetic_boundary_u256() -> PartialVMResult<()> {
    assert!(
        IntegerValue::U256(U256::max_value())
            .add_checked(IntegerValue::U256(U256::from(1u64)))
            .is_err()
    );
    assert_eq!(
        IntegerValue::U256(U256::max_value())
            .add_checked(IntegerValue::U256(U256::zero()))?
            .cast_u256()?,
        U256::max_value()
    );
    assert!(
        IntegerValue::U256(U256::zero())
            .sub_checked(IntegerValue::U256(U256::from(1u64)))
            .is_err()
    );
    assert_eq!(
        IntegerValue::U256(U256::max_value())
            .sub_checked(IntegerValue::U256(U256::max_value()))?
            .cast_u256()?,
        U256::zero()
    );
    assert!(
        IntegerValue::U256(U256::max_value())
            .mul_checked(IntegerValue::U256(U256::from(2u64)))
            .is_err()
    );
    assert_eq!(
        IntegerValue::U256(U256::max_value())
            .mul_checked(IntegerValue::U256(U256::from(1u64)))?
            .cast_u256()?,
        U256::max_value()
    );
    assert!(
        IntegerValue::U256(U256::from(1u64))
            .div_checked(IntegerValue::U256(U256::zero()))
            .is_err()
    );
    assert!(
        IntegerValue::U256(U256::from(1u64))
            .rem_checked(IntegerValue::U256(U256::zero()))
            .is_err()
    );
    Ok(())
}

/// Shift overflow at exact bit-width boundary for every integer type.
#[test]
fn integer_shift_boundary_all_widths() -> PartialVMResult<()> {
    // Shifting by exactly the bit-width errors; shifting by bit-width - 1 succeeds.
    assert!(IntegerValue::U8(1).shl_checked(8).is_err());
    assert!(IntegerValue::U8(1).shl_checked(7).is_ok());
    assert!(IntegerValue::U16(1).shl_checked(16).is_err());
    assert!(IntegerValue::U16(1).shl_checked(15).is_ok());
    assert!(IntegerValue::U32(1).shl_checked(32).is_err());
    assert!(IntegerValue::U32(1).shl_checked(31).is_ok());
    assert!(IntegerValue::U64(1).shl_checked(64).is_err());
    assert!(IntegerValue::U64(1).shl_checked(63).is_ok());
    assert!(IntegerValue::U128(1).shl_checked(128).is_err());
    assert!(IntegerValue::U128(1).shl_checked(127).is_ok());
    // U256 has 256 bits but shift amount is u8 (max 255), so max valid shift is 255.
    assert!(
        IntegerValue::U256(U256::from(1u64))
            .shl_checked(255)
            .is_ok()
    );
    // Same for shr
    assert!(IntegerValue::U8(1).shr_checked(8).is_err());
    assert!(IntegerValue::U8(1).shr_checked(7).is_ok());
    assert!(IntegerValue::U16(1).shr_checked(16).is_err());
    assert!(IntegerValue::U64(1).shr_checked(64).is_err());
    assert!(IntegerValue::U128(1).shr_checked(128).is_err());
    assert!(
        IntegerValue::U256(U256::from(1u64))
            .shr_checked(255)
            .is_ok()
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Integer Cast Boundary Conditions — exact MAX/MAX+1 at every type boundary.
// ---------------------------------------------------------------------------

/// Casting exactly T::MAX to a wider type succeeds; T::MAX+1 in a narrower cast fails.
#[test]
fn integer_cast_boundary_u8() -> PartialVMResult<()> {
    // u8::MAX fits in all wider types
    assert_eq!(IntegerValue::U8(u8::MAX).cast_u16()?, u8::MAX as u16);
    assert_eq!(IntegerValue::U8(u8::MAX).cast_u32()?, u8::MAX as u32);
    assert_eq!(IntegerValue::U8(u8::MAX).cast_u64()?, u8::MAX as u64);
    assert_eq!(IntegerValue::U8(u8::MAX).cast_u128()?, u8::MAX as u128);
    assert_eq!(
        IntegerValue::U8(u8::MAX).cast_u256()?,
        U256::from(u8::MAX as u64)
    );
    // u8::MAX cast back to u8 succeeds (identity)
    assert_eq!(IntegerValue::U8(u8::MAX).cast_u8()?, u8::MAX);
    // 0 always succeeds
    assert_eq!(IntegerValue::U8(0).cast_u8()?, 0u8);
    Ok(())
}

/// u16 boundary: MAX fits in wider types, MAX narrows to u8 fails.
#[test]
fn integer_cast_boundary_u16() -> PartialVMResult<()> {
    assert_eq!(IntegerValue::U16(u16::MAX).cast_u32()?, u16::MAX as u32);
    assert_eq!(IntegerValue::U16(u16::MAX).cast_u64()?, u16::MAX as u64);
    assert_eq!(IntegerValue::U16(u16::MAX).cast_u128()?, u16::MAX as u128);
    assert_eq!(
        IntegerValue::U16(u16::MAX).cast_u256()?,
        U256::from(u16::MAX as u64)
    );
    assert_eq!(IntegerValue::U16(u16::MAX).cast_u16()?, u16::MAX);
    // u16::MAX > u8::MAX → narrowing fails
    assert!(IntegerValue::U16(u16::MAX).cast_u8().is_err());
    // u8::MAX as u16 → narrowing succeeds
    assert_eq!(IntegerValue::U16(u8::MAX as u16).cast_u8()?, u8::MAX);
    // u8::MAX + 1 as u16 → narrowing fails
    assert!(IntegerValue::U16(u8::MAX as u16 + 1).cast_u8().is_err());
    Ok(())
}

/// u32 boundary: MAX fits in wider types, narrowing to u8/u16 fails at boundary.
#[test]
fn integer_cast_boundary_u32() -> PartialVMResult<()> {
    assert_eq!(IntegerValue::U32(u32::MAX).cast_u64()?, u32::MAX as u64);
    assert_eq!(IntegerValue::U32(u32::MAX).cast_u128()?, u32::MAX as u128);
    assert_eq!(
        IntegerValue::U32(u32::MAX).cast_u256()?,
        U256::from(u32::MAX as u64)
    );
    assert_eq!(IntegerValue::U32(u32::MAX).cast_u32()?, u32::MAX);
    // Narrowing failures
    assert!(IntegerValue::U32(u32::MAX).cast_u8().is_err());
    assert!(IntegerValue::U32(u32::MAX).cast_u16().is_err());
    // Exact boundary: u16::MAX fits, u16::MAX+1 doesn't
    assert_eq!(IntegerValue::U32(u16::MAX as u32).cast_u16()?, u16::MAX);
    assert!(IntegerValue::U32(u16::MAX as u32 + 1).cast_u16().is_err());
    assert_eq!(IntegerValue::U32(u8::MAX as u32).cast_u8()?, u8::MAX);
    assert!(IntegerValue::U32(u8::MAX as u32 + 1).cast_u8().is_err());
    Ok(())
}

/// u64 boundary: MAX fits in u128/u256, narrowing to u8/u16/u32 fails at boundary.
#[test]
fn integer_cast_boundary_u64() -> PartialVMResult<()> {
    assert_eq!(IntegerValue::U64(u64::MAX).cast_u128()?, u64::MAX as u128);
    assert_eq!(
        IntegerValue::U64(u64::MAX).cast_u256()?,
        U256::from(u64::MAX)
    );
    assert_eq!(IntegerValue::U64(u64::MAX).cast_u64()?, u64::MAX);
    // Narrowing failures at MAX
    assert!(IntegerValue::U64(u64::MAX).cast_u8().is_err());
    assert!(IntegerValue::U64(u64::MAX).cast_u16().is_err());
    assert!(IntegerValue::U64(u64::MAX).cast_u32().is_err());
    // Exact boundaries
    assert_eq!(IntegerValue::U64(u32::MAX as u64).cast_u32()?, u32::MAX);
    assert!(IntegerValue::U64(u32::MAX as u64 + 1).cast_u32().is_err());
    assert_eq!(IntegerValue::U64(u16::MAX as u64).cast_u16()?, u16::MAX);
    assert!(IntegerValue::U64(u16::MAX as u64 + 1).cast_u16().is_err());
    assert_eq!(IntegerValue::U64(u8::MAX as u64).cast_u8()?, u8::MAX);
    assert!(IntegerValue::U64(u8::MAX as u64 + 1).cast_u8().is_err());
    Ok(())
}

/// u128 boundary: MAX fits in u256, narrowing to all smaller types fails at boundary.
#[test]
fn integer_cast_boundary_u128() -> PartialVMResult<()> {
    assert_eq!(
        IntegerValue::U128(u128::MAX).cast_u256()?,
        U256::from(u128::MAX)
    );
    assert_eq!(IntegerValue::U128(u128::MAX).cast_u128()?, u128::MAX);
    // Narrowing failures at MAX
    assert!(IntegerValue::U128(u128::MAX).cast_u8().is_err());
    assert!(IntegerValue::U128(u128::MAX).cast_u16().is_err());
    assert!(IntegerValue::U128(u128::MAX).cast_u32().is_err());
    assert!(IntegerValue::U128(u128::MAX).cast_u64().is_err());
    // Exact boundaries
    assert_eq!(IntegerValue::U128(u64::MAX as u128).cast_u64()?, u64::MAX);
    assert!(IntegerValue::U128(u64::MAX as u128 + 1).cast_u64().is_err());
    assert_eq!(IntegerValue::U128(u32::MAX as u128).cast_u32()?, u32::MAX);
    assert!(IntegerValue::U128(u32::MAX as u128 + 1).cast_u32().is_err());
    assert_eq!(IntegerValue::U128(u16::MAX as u128).cast_u16()?, u16::MAX);
    assert!(IntegerValue::U128(u16::MAX as u128 + 1).cast_u16().is_err());
    assert_eq!(IntegerValue::U128(u8::MAX as u128).cast_u8()?, u8::MAX);
    assert!(IntegerValue::U128(u8::MAX as u128 + 1).cast_u8().is_err());
    Ok(())
}

/// u256 boundary: narrowing to all smaller types fails for values above their MAX.
#[test]
fn integer_cast_boundary_u256() -> PartialVMResult<()> {
    // Identity
    assert!(IntegerValue::U256(U256::max_value()).cast_u256().is_ok());
    assert_eq!(IntegerValue::U256(U256::zero()).cast_u256()?, U256::zero());
    // Narrowing failures at MAX+1
    assert!(
        IntegerValue::U256(U256::from(u128::MAX) + U256::from(1u64))
            .cast_u128()
            .is_err()
    );
    assert!(
        IntegerValue::U256(U256::from(u64::MAX) + U256::from(1u64))
            .cast_u64()
            .is_err()
    );
    assert!(
        IntegerValue::U256(U256::from(u32::MAX as u64) + U256::from(1u64))
            .cast_u32()
            .is_err()
    );
    assert!(
        IntegerValue::U256(U256::from(u16::MAX as u64) + U256::from(1u64))
            .cast_u16()
            .is_err()
    );
    assert!(
        IntegerValue::U256(U256::from(u8::MAX as u64) + U256::from(1u64))
            .cast_u8()
            .is_err()
    );
    // Exact MAX fits
    assert_eq!(
        IntegerValue::U256(U256::from(u128::MAX)).cast_u128()?,
        u128::MAX
    );
    assert_eq!(
        IntegerValue::U256(U256::from(u64::MAX)).cast_u64()?,
        u64::MAX
    );
    assert_eq!(
        IntegerValue::U256(U256::from(u32::MAX as u64)).cast_u32()?,
        u32::MAX
    );
    assert_eq!(
        IntegerValue::U256(U256::from(u16::MAX as u64)).cast_u16()?,
        u16::MAX
    );
    assert_eq!(
        IntegerValue::U256(U256::from(u8::MAX as u64)).cast_u8()?,
        u8::MAX
    );
    // Zero always works
    assert_eq!(IntegerValue::U256(U256::zero()).cast_u8()?, 0u8);
    assert_eq!(IntegerValue::U256(U256::zero()).cast_u128()?, 0u128);
    Ok(())
}

// ---------------------------------------------------------------------------
// VMValueCast Error Paths — casting to incompatible types.
// ---------------------------------------------------------------------------

/// Primitives cannot be cast to unrelated types (u64 -> Struct, etc.).
#[test]
fn cast_wrong_type_errors() {
    assert!(VMValueCast::<Struct>::cast(Value::u64(0)).is_err());
    assert!(VMValueCast::<u8>::cast(Value::bool(true)).is_err());
    assert!(VMValueCast::<u64>::cast(Value::address(AccountAddress::ZERO)).is_err());
}

/// An indexed ref (into a PrimVec) cannot be cast to StructRef.
#[test]
fn cast_indexed_ref_to_struct_ref_errors() -> PartialVMResult<()> {
    let v = MemBox::new(Value::vector_u8([1, 2, 3]));
    let vr: VectorRef = VMValueCast::cast(v.as_ref_value())?;
    let elem = vr.borrow_elem(0, &Type::U8)?;

    assert!(VMValueCast::<StructRef>::cast(elem).is_err());
    Ok(())
}

// ---------------------------------------------------------------------------
// VectorSpecialization — TryFrom<&Type> mapping.
// ---------------------------------------------------------------------------

/// Each Type maps to the expected specialization; ref/typaram types error.
#[test]
fn vector_specialization_from_type() {
    use VectorSpecialization as VS;

    assert!(matches!(VS::try_from(&Type::U8), Ok(VS::U8)));
    assert!(matches!(VS::try_from(&Type::U16), Ok(VS::U16)));
    assert!(matches!(VS::try_from(&Type::U32), Ok(VS::U32)));
    assert!(matches!(VS::try_from(&Type::U64), Ok(VS::U64)));
    assert!(matches!(VS::try_from(&Type::U128), Ok(VS::U128)));
    assert!(matches!(VS::try_from(&Type::U256), Ok(VS::U256)));
    assert!(matches!(VS::try_from(&Type::Bool), Ok(VS::Bool)));
    assert!(matches!(VS::try_from(&Type::Address), Ok(VS::Address)));
    assert!(matches!(VS::try_from(&Type::Signer), Ok(VS::Container)));
    assert!(matches!(
        VS::try_from(&Type::Vector(Box::new(Type::U8))),
        Ok(VS::Container)
    ));
    assert!(VS::try_from(&Type::Reference(Box::new(Type::U8))).is_err());
    assert!(VS::try_from(&Type::TyParam(0)).is_err());
}
