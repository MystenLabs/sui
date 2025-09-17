// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    execution::{interpreter::locals::MachineHeap, values::*},
    jit::execution::ast::Type,
    shared::views::*,
};
use move_binary_format::errors::*;
use move_core_types::{account_address::AccountAddress, runtime_value, u256::U256};

#[cfg(test)]
const SIZE_CONFIG: SizeConfig = SizeConfig {
    traverse_references: false,
    include_vector_size: false,
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
    let unpacked: Vec<_> = s.unpack()?.collect();

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

#[test]
fn legacy_ref_abstract_memory_size_consistency() -> PartialVMResult<()> {
    #![allow(deprecated)]
    let mut heap = MachineHeap::new();
    let mut locals = heap.allocate_stack_frame(vec![], 10)?;

    locals.store_loc(0, Value::u128(0))?;
    let r = locals.borrow_loc(0)?;
    assert_eq!(r.abstract_memory_size(&SIZE_CONFIG), r.legacy_size());

    locals.store_loc(1, Value::vector_u8([1, 2, 3]))?;
    let r = locals.borrow_loc(1)?;
    assert_eq!(r.abstract_memory_size(&SIZE_CONFIG), r.legacy_size());

    let r: VectorRef = VMValueCast::cast(r)?;
    let r = r.borrow_elem(0, &Type::U8)?;
    assert_eq!(r.abstract_memory_size(&SIZE_CONFIG), r.legacy_size());

    locals.store_loc(2, Value::make_struct(vec![]))?;
    let r: Reference = VMValueCast::cast(locals.borrow_loc(2)?)?;
    assert_eq!(r.abstract_memory_size(&SIZE_CONFIG), r.legacy_size());

    Ok(())
}

#[test]
fn legacy_struct_abstract_memory_size_consistenty() -> PartialVMResult<()> {
    let structs = [
        Struct::pack([]),
        Struct::pack([Value::make_struct(vec![Value::u8(0), Value::u64(0)])]),
    ];

    for s in &structs {
        assert_eq!(s.abstract_memory_size(&SIZE_CONFIG), s.legacy_size());
    }

    Ok(())
}

#[test]
fn legacy_val_abstract_memory_size_consistency() -> PartialVMResult<()> {
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
        Value::vector_u256([1, 2, 3, 4].iter().map(|q| U256::from(*q as u64))),
        Value::make_struct([]),
        Value::make_struct([Value::u8(0), Value::bool(false)]),
        Vector::pack(VectorSpecialization::Container, [])?,
        Vector::pack(VectorSpecialization::U8, [Value::u8(0), Value::u8(1)])?,
    ];

    let mut heap = MachineHeap::new();
    let mut locals = heap.allocate_stack_frame(vec![], vals.len())?;

    for (idx, val) in vals.iter().enumerate() {
        locals.store_loc(idx, val.copy_value())?;

        let val_size_new = val.abstract_memory_size(&SIZE_CONFIG);
        #[allow(deprecated)]
        let val_size_old = val.legacy_size();

        assert_eq!(val_size_new, val_size_old);

        let ref_: Reference = VMValueCast::cast(locals.borrow_loc(idx)?)?;
        let val_size_through_ref = ref_.value_view().abstract_memory_size(&SIZE_CONFIG);

        assert_eq!(
            val_size_through_ref, val_size_old,
            "{:?} (ref size {} != old size {})",
            val, val_size_through_ref, val_size_old
        );
    }

    Ok(())
}

#[test]
fn test_vm_value_vector_u64_casting() {
    assert_eq!(
        vec![1, 2, 3],
        VMValueCast::<Vec<u64>>::cast(Value::vector_u64([1, 2, 3])).unwrap()
    );
}

// TODO(bella-ciao-merge): Make sure we are happy with this size change (from 16 -> 32).
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
