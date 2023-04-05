module Symbols::M1 {

    struct SomeStruct has key, drop, store {
        some_field: u64,
    }

    const SOME_CONST: u64 = 42;


    fun unpack(s: SomeStruct): u64 {
        let SomeStruct { some_field: value } = s;
        value
    }

    fun cp(value: u64): u64 {
        let ret = value;
        ret
    }

    fun pack(): SomeStruct {
        let ret = SomeStruct { some_field: SOME_CONST };
        ret
    }

    fun other_mod_struct(): Symbols::M2::SomeOtherStruct {
        Symbols::M2::some_other_struct(SOME_CONST)
    }

    use Symbols::M2::{Self, SomeOtherStruct};

    fun other_mod_struct_import(): SomeOtherStruct {
        M2::some_other_struct(7)
    }

    fun acq(addr: address): u64 acquires SomeStruct {
        let val = borrow_global<SomeStruct>(addr);
        val.some_field
    }

    fun multi_arg_call(): u64 {
        M2::multi_arg(SOME_CONST, SOME_CONST)
    }

    fun vec(): vector<SomeStruct> {
        let s = SomeStruct{ some_field: 7 };
        vector<SomeStruct>[SomeStruct{ some_field: 42 }, s]
    }

    fun unpack_no_assign(s: SomeStruct): u64 {
        let value: u64;
        SomeStruct { some_field: value } = s;
        value
    }

    fun mut(): u64 {
        let tmp = 7;
        let r = &mut tmp;
        *r = SOME_CONST;
        tmp
    }

    fun ret(p1: bool, p2: u64): u64 {
        if (p1) {
            return SOME_CONST
        };
        p2
    }

    fun abort_call() {
        abort SOME_CONST
    }

    fun deref(): u64 {
        let tmp = 7;
        let r = &tmp;
        *r
    }

    fun unary(p: bool):bool {
        !p
    }

    fun temp_borrow(): u64 {
        let tmp = &SOME_CONST;
        *tmp
    }

    struct OuterStruct has key, drop {
        some_struct: SomeStruct,
    }

    fun chain_access(): u64 {
        let inner = SomeStruct{ some_field: 42 };
        let outer = OuterStruct{ some_struct: inner };
        outer.some_struct.some_field
    }

    fun chain_access_block(): u64 {
        let inner = SomeStruct{ some_field: 42 };
        let outer = OuterStruct{ some_struct: inner };
        {
            outer
        }.some_struct.some_field
    }

    fun chain_access_borrow(): u64 {
        let inner = SomeStruct{ some_field: 42 };
        let outer = OuterStruct{ some_struct: inner };
        let r = &outer.some_struct.some_field;
        *r
    }

    fun cast(): u64 {
        let tmp: u128 = 42;
        (tmp as u64)
    }

    fun annot(): u64 {
        let tmp = (SOME_CONST: u64);
        tmp
    }

    fun struct_param(p: SomeOtherStruct): SomeOtherStruct {
        p
    }

    fun struct_var(p: bool): SomeOtherStruct {
        let tmp = M2::some_other_struct(7);
        if (p) {
            tmp
        } else {
            M2::some_other_struct(42)
        }
    }

}
