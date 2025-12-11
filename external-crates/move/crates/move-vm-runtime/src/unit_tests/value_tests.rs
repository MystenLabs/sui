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
use move_binary_format::errors::*;
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
        let val_size = val.abstract_memory_size(&SIZE_CONFIG);
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
    let val_size = r.abstract_memory_size(&SIZE_CONFIG);
    writeln!(&mut output, "size of {:?}: {}, ", print_ref(&r), val_size).unwrap();

    let val_size_traverse = r.abstract_memory_size(&SIZE_CONFIG_TRAVERSE);
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
            s.abstract_memory_size(&SIZE_CONFIG),
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
        Value::vector_u256([1, 2, 3, 4].iter().map(|q| U256::from(*q as u64))),
        Value::make_struct([]),
        Value::make_struct([Value::u8(0), Value::bool(false)]),
        Vector::pack(VectorSpecialization::Container, [])?,
        Vector::pack(VectorSpecialization::U8, [Value::u8(0), Value::u8(1)])?,
    ];

    let mut heap = MachineHeap::new();
    let mut locals = heap.allocate_stack_frame(vec![], vals.len())?;

    let record_val_size = |output: &mut String, val: &Value| {
        let val_size = val.abstract_memory_size(&SIZE_CONFIG);
        writeln!(output, "size of {:?}: {}, ", print_val(val), val_size).unwrap();
    };

    let record_ref_size = |output: &mut String, val: &Reference| {
        let val_size = val.abstract_memory_size(&SIZE_CONFIG);
        let val_size_traverse = val.abstract_memory_size(&SIZE_CONFIG_TRAVERSE);
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
