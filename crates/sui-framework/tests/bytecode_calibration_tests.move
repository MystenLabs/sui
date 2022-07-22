// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0


// This module attemps to find the computational cost of bytecode instructions by measuring the time
// the instruction takes to execute.
// Isolating the bytecode is tricky, so we run two functions with and without the bytecode instruction
// The difference in execution times is the time the instruction takes
// functions prefixed with __baseline do not have the bytecode under yesy
// Many parts of the code are written in such a way that the bytecode diffs yield exactly the 
// instruction/operation to be isolated


#[test_only]
module sui::bytecode_calibration_tests {
    use std::vector;

    // Number of times to run the inner loop of tests
    // We set this value 1 to avoid long running tests
    // But normally we want something like 1000000
    const NUM_TRIALS: u64 = 1;
    const U64_MAX: u64 = 18446744073709551615;

    struct ObjectWithU8Field has store, drop{
        f0: u8,
    }

    struct ObjectWithU64Field has store, drop{
        f0: u64,
    }

    // Add operation
    #[test]
    public entry fun test_calibrate_add() {
        let trials: u64 = NUM_TRIALS;
        let num: u64 = 0;
        while (trials > 0) {
            num = num + 1 + 1;
            trials = trials - 1;
        }
    }
    #[test]
    public entry fun test_calibrate_add__baseline() {
        let trials: u64 = NUM_TRIALS;
        let num: u64 = 0;
        while (trials > 0) {
            num = num + 1;
            trials = trials - 1;
        }
    }

    // Sub operation
    #[test]
    public entry fun test_calibrate_sub() {
        let trials: u64 = NUM_TRIALS;
        let num: u64 = U64_MAX;
        while (trials > 0) {
            num = num - 1 - 1;
            trials = trials - 1;
        }
    }
    #[test]
    public entry fun test_calibrate_sub__baseline() {
        let trials: u64 = NUM_TRIALS;
        let num: u64 = U64_MAX;
        while (trials > 0) {
            num = num - 1;
            trials = trials - 1;
        }
    }

    // Mul operation
    #[test]
    public entry fun test_calibrate_mul() {
        let trials: u64 = NUM_TRIALS;
        let num: u64 = 0;
        while (trials > 0) {
            num = num * 1 * 1;
            trials = trials - 1;
        }
    }
    #[test]
    public entry fun test_calibrate_mul__baseline() {
        let trials: u64 = NUM_TRIALS;
        let num: u64 = 0;
        while (trials > 0) {
            num = num * 1;
            trials = trials - 1;
        }
    }

    // Div operation
    #[test]
    public entry fun test_calibrate_div() {
        let trials: u64 = NUM_TRIALS;
        let num: u64 = U64_MAX;
        while (trials > 0) {
            num = num / 1 / 1;
            trials = trials - 1;
        }
    }
    #[test]
    public entry fun test_calibrate_div__baseline() {
        let trials: u64 = NUM_TRIALS;
        let num: u64 = U64_MAX;
        while (trials > 0) {
            num = num / 1;
            trials = trials - 1;
        }
    }

    // Mod operation
    #[test]
    public entry fun test_calibrate_mod() {
        let trials: u64 = NUM_TRIALS;
        let num: u64 = 0;
        while (trials > 0) {
            num = num % 1 % 1;
            trials = trials - 1;
        }
    }
    #[test]
    public entry fun test_calibrate_mod__baseline() {
        let trials: u64 = NUM_TRIALS;
        let num: u64 = 0;
        while (trials > 0) {
            num = num % 1;
            trials = trials - 1;
        }
    }

    // Logical And operation
    #[test]
    public entry fun test_calibrate_and() {
        let trials: u64 = NUM_TRIALS;
        let flag: bool = false;
        while (trials > 0) {
            flag = flag && false && false;
            trials = trials - 1;
        }
    }
    #[test]
    public entry fun test_calibrate_and__baseline() {
        let trials: u64 = NUM_TRIALS;
        let flag: bool = false;
        while (trials > 0) {
            flag = flag && false;
            trials = trials - 1;
        }
    }

    // Logical Or operation
    #[test]
    public entry fun test_calibrate_or() {
        let trials: u64 = NUM_TRIALS;
        let flag: bool = false;
        while (trials > 0) {
            flag = flag || false || false;
            trials = trials - 1;
        }
    }
    #[test]
    public entry fun test_calibrate_or__baseline() {
        let trials: u64 = NUM_TRIALS;
        let flag: bool = false;
        while (trials > 0) {
            flag = flag || false;
            trials = trials - 1;
        }
    }

    // Xor operation
    #[test]
    public entry fun test_calibrate_xor() {
        let trials: u64 = NUM_TRIALS;
        let num: u64 = U64_MAX;
        while (trials > 0) {
            num = num ^ 1 ^ 1;
            trials = trials - 1;
        }
    }
    #[test]
    public entry fun test_calibrate_xor__baseline() {
        let trials: u64 = NUM_TRIALS;
        let num: u64 = U64_MAX;
        while (trials > 0) {
            num = num ^ 1;
            trials = trials - 1;
        }
    }

    // Shift Right operation
    #[test]
    public entry fun test_calibrate_shr() {
        let trials: u64 = NUM_TRIALS;
        let num: u64 = U64_MAX;
        while (trials > 0) {
            num = num >> 1 >> 1;
            trials = trials - 1;
        }
    }
    #[test]
    public entry fun test_calibrate_shr__baseline() {
        let trials: u64 = NUM_TRIALS;
        let num: u64 = U64_MAX;
        while (trials > 0) {
            num = num >> 1;
            trials = trials - 1;
        }
    }

    // Shift num operation
    #[test]
    public entry fun test_calibrate_shl() {
        let trials: u64 = NUM_TRIALS;
        let num: u64 = U64_MAX;
        while (trials > 0) {
            num = num << 1 << 1;
            trials = trials - 1;
        }
    }
    #[test]
    public entry fun test_calibrate_shl__baseline() {
        let trials: u64 = NUM_TRIALS;
        let num: u64 = U64_MAX;
        while (trials > 0) {
            num = num << 1;
            trials = trials - 1;
        }
    }

    // Bitwise And operation
    #[test]
    public entry fun test_calibrate_bitand() {
        let trials: u64 = NUM_TRIALS;
        let num: u64 = U64_MAX;
        while (trials > 0) {
            num = num & 1 & 1;
            trials = trials - 1;
        }
    }
    #[test]
    public entry fun test_calibrate_bitand__baseline() {
        let trials: u64 = NUM_TRIALS;
        let num: u64 = U64_MAX;
        while (trials > 0) {
            num = num & 1;
            trials = trials - 1;
        }
    }

    // Bitwise Or operation
    #[test]
    public entry fun test_calibrate_bitor() {
        let trials: u64 = NUM_TRIALS;
        let num: u64 = U64_MAX;
        while (trials > 0) {
            num = num | 1 | 1;
            trials = trials - 1;
        }
    }
    #[test]
    public entry fun test_calibrate_bitor__baseline() {
        let trials: u64 = NUM_TRIALS;
        let num: u64 = U64_MAX;
        while (trials > 0) {
            num = num | 1;
            trials = trials - 1;
        }
    }

    // Eq operation
    #[test]
    public entry fun test_calibrate_eq() {
        let trials: u64 = NUM_TRIALS;
        let flag: bool = false;
        while (trials > 0) {
            flag = flag == true == true;
            trials = trials - 1;
        }
    }
    #[test]
    public entry fun test_calibrate_eq__baseline() {
        let trials: u64 = NUM_TRIALS;
        let flag: bool = false;
        while (trials > 0) {
            flag = flag == true;
            trials = trials - 1;
        }
    }

    // Neq operation
    #[test]
    public entry fun test_calibrate_neq() {
        let trials: u64 = NUM_TRIALS;
        let flag: bool = false;
        while (trials > 0) {
            flag = flag != true != true;
            trials = trials - 1;
        }
    }
    #[test]
    public entry fun test_calibrate_neq__baseline() {
        let trials: u64 = NUM_TRIALS;
        let flag: bool = false;
        while (trials > 0) {
            flag = flag != true;
            trials = trials - 1;
        }
    }

    // Lt operation
    #[test]
    public entry fun test_calibrate_lt() {
        let trials: u64 = NUM_TRIALS;
        let _flag: bool;
        while (trials > 0) {
            _flag = trials < 1;
            trials = trials - 1;
        }
    }
    // Lt operation
    #[test]
    public entry fun test_calibrate_lt__baseline() {
        let trials: u64 = NUM_TRIALS;
        let _flag: bool;
        while (trials > 0) {
            let _ = trials;
            trials = trials - 1;
        }
    }

    // Gt operation
    #[test]
    public entry fun test_calibrate_gt() {
        let trials: u64 = NUM_TRIALS;
        let _flag: bool;
        while (trials > 0) {
            _flag = trials > 1;
            trials = trials - 1;
        }
    }
    // Gt operation
    #[test]
    public entry fun test_calibrate_gt__baseline() {
        let trials: u64 = NUM_TRIALS;
        let _flag: bool;
        while (trials > 0) {
            let _ = trials;
            trials = trials - 1;
        }
    }

    // Le operation
    #[test]
    public entry fun test_calibrate_le() {
        let trials: u64 = NUM_TRIALS;
        let _flag: bool;
        while (trials > 0) {
            _flag = trials <= 1;
            trials = trials - 1;
        }
    }
    // Le operation
    #[test]
    public entry fun test_calibrate_le__baseline() {
        let trials: u64 = NUM_TRIALS;
        let _flag: bool;
        while (trials > 0) {
            let _ = trials;
            trials = trials - 1;
        }
    }

    // Ge operation
    #[test]
    public entry fun test_calibrate_ge() {
        let trials: u64 = NUM_TRIALS;
        let _flag: bool;
        while (trials > 0) {
            _flag = trials >= 1;
            trials = trials - 1;
        }
    }
    // Ge operation
    #[test]
    public entry fun test_calibrate_ge__baseline() {
        let trials: u64 = NUM_TRIALS;
        let _flag: bool;
        while (trials > 0) {
            let _ = trials;
            trials = trials - 1;
        }
    }

    // Not operation
    #[test]
    public entry fun test_calibrate_not() {
        let trials: u64 = NUM_TRIALS;
        let _flag: bool = false;
        while (trials > 0) {
            _flag = !!_flag;
            trials = trials - 1;
        }
    }
    // Not operation
    #[test]
    public entry fun test_calibrate_not__baseline() {
        let trials: u64 = NUM_TRIALS;
        let _flag: bool = false;
        while (trials > 0) {
            _flag = !_flag;
            trials = trials - 1;
        }
    }


    // =================
    // Memory access

    // Immutable Borrow of local operation
    #[test]
    public entry fun test_calibrate_imm_borrow_loc() {
        let trials: u64 = NUM_TRIALS;
        while (trials > 0) {
            let _r = &trials;
            trials = trials - 1;
        }
    }
    #[test]
    public entry fun test_calibrate_imm_borrow_loc__baseline() {
        let trials: u64 = NUM_TRIALS;
        while (trials > 0) {
            trials = trials - 1;
        }
    }
    // Mutable Borrow of local operation
    #[test]
    public entry fun test_calibrate_mut_borrow_loc() {
        let trials: u64 = NUM_TRIALS;
        while (trials > 0) {
            let _r = &mut trials;
            trials = trials - 1;
        }
    }
    #[test]
    public entry fun test_calibrate_mut_borrow_loc__baseline() {
        let trials: u64 = NUM_TRIALS;
        while (trials > 0) {
            trials = trials - 1;
        }
    }


    // Immutable Borrow of field operation
    #[test]
    public entry fun test_calibrate_imm_borrow_field() {
        let trials: u64 = NUM_TRIALS;
        let obj = ObjectWithU64Field {f0: 0u64};
        while (trials > 0) {
            let _r = &obj.f0;
            trials = trials - 1;
        }
    }
    #[test]
    public entry fun test_calibrate_imm_borrow_field__baseline() {
        let trials: u64 = NUM_TRIALS;
        let obj = ObjectWithU64Field {f0: 0u64};
        while (trials > 0) {
            let _r = &obj;
            trials = trials - 1;
        }
    }
    // Mutable Borrow of local operation
    #[test]
    public entry fun test_calibrate_mut_borrow_field() {
        let trials: u64 = NUM_TRIALS;
        let obj = ObjectWithU64Field {f0: 0u64};
        while (trials > 0) {
            let _r = &mut obj.f0;
            trials = trials - 1;
        }
    }
    #[test]
    public entry fun test_calibrate_mut_borrow_field__baseline() {
        let trials: u64 = NUM_TRIALS;
        let obj = ObjectWithU64Field {f0: 0u64};
        while (trials > 0) {
            let _r = &mut obj;
            trials = trials - 1;
        }
    }


    #[test]
    public entry fun test_calibrate_ldu8() {
        let trials: u64 = NUM_TRIALS;
        let _num = 0u8;
        while (trials > 0) {
            trials = trials - 1;
            _num = 0u8;
        }
    }
    #[test]
    public entry fun test_calibrate_ldu8__baseline() {
        let trials: u64 = NUM_TRIALS;
        let _num = 0u8;
        while (trials > 0) {
            trials = trials - 1;
        }
    }
    #[test]
    public entry fun test_calibrate_ldu64() {
        let trials: u64 = NUM_TRIALS;
        let _num = 0u64;
        while (trials > 0) {
            trials = trials - 1;
            _num = 0u64;
        }
    }
    #[test]
    public entry fun test_calibrate_ldu64__baseline() {
        let trials: u64 = NUM_TRIALS;
        let _num = 0u64;
        while (trials > 0) {
            trials = trials - 1;
        }
    }
    #[test]
    public entry fun test_calibrate_ldu128() {
        let trials: u64 = NUM_TRIALS;
        let _num = 0u128;
        while (trials > 0) {
            trials = trials - 1;
            _num = 0u128;
        }
    }
    #[test]
    public entry fun test_calibrate_ldu128__baseline() {
        let trials: u64 = NUM_TRIALS;
        let _num = 0u128;
        while (trials > 0) {
            trials = trials - 1;
        }
    }

    #[test]
    public entry fun test_calibrate_ld_const() {
        let trials: u64 = NUM_TRIALS;
        let _num = 0u64;
        while (trials > 0) {
            trials = trials - 1;
            _num = U64_MAX;
        }
    }
    #[test]
    public entry fun test_calibrate_ld_const__baseline() {
        let trials: u64 = NUM_TRIALS;
        let _num = 0u64;
        while (trials > 0) {
            trials = trials - 1;
        }
    }

    #[test]
    public entry fun test_calibrate_pack() {
        let trials: u64 = NUM_TRIALS;
        let _num: ObjectWithU8Field;
        while (trials > 0) {
            trials = trials - 1;
            _num = ObjectWithU8Field {f0: 0u8};
        }
    }

    #[test]
    public entry fun test_calibrate_pack__baseline() {
        let trials: u64 = NUM_TRIALS;
        let _num: ObjectWithU8Field;
        while (trials > 0) {
            trials = trials - 1;
            // This forces a u8 load to counter that in subject
            let _ = 0u8;
        }
    }

    #[test]
    public entry fun test_calibrate_unpack() {
        let trials: u64 = NUM_TRIALS;
        let _num: ObjectWithU8Field;
        while (trials > 0) {
            trials = trials - 1;
            _num = ObjectWithU8Field {f0: 0u8};
            let ObjectWithU8Field { f0: _ } = _num;
        }
    }

    #[test]
    public entry fun test_calibrate_unpack__baseline() {
        let trials: u64 = NUM_TRIALS;
        let _num: ObjectWithU8Field;
        while (trials > 0) {
            trials = trials - 1;
            _num = ObjectWithU8Field {f0: 0u8};
        }
    }


    #[test]
    public entry fun test_calibrate_read_ref() {
        let trials: u64 = NUM_TRIALS;
        let r = 0u64;
        while (trials > 0) {
            let _ = *&r;
            trials = trials - 1;
        }
    }

    #[test]
    public entry fun test_calibrate_read_ref__baseline() {
        let trials: u64 = NUM_TRIALS;
        let r = 0u64;
        while (trials > 0) {
            let _ = &r;
            trials = trials - 1;
        }
    }

    #[test]
    public entry fun test_calibrate_write_ref() {
        let trials: u64 = NUM_TRIALS;
        let r = 0u64;
        while (trials > 0) {
            *(&mut r) = trials;
            trials = trials - 1;
        }
    }

    #[test]
    public entry fun test_calibrate_write_ref__baseline() {
        let trials: u64 = NUM_TRIALS;
        let r = 0u64;
        while (trials > 0) {
            let _ = trials;
            let _ = &mut r;
            trials = trials - 1;
        }
    }


    #[test]
    public entry fun test_calibrate_copy_loc() {
        let trials: u64 = NUM_TRIALS;
        while (trials > 0) {
            let _ = trials;
            trials = trials - 1;
        }
    }

    #[test]
    public entry fun test_calibrate_copy_loc__baseline() {
        let trials: u64 = NUM_TRIALS;
        while (trials > 0) {
            trials = trials - 1;
        }
    }

    #[test]
    public entry fun test_calibrate_vec_len() {
        let trials: u64 = NUM_TRIALS;
        while (trials > 0) {
            let hash = x"0134";
            vector::length(&hash);
            trials = trials - 1;
        }
    }

    #[test]
    public entry fun test_calibrate_vec_len__baseline() {
        let trials: u64 = NUM_TRIALS;
        while (trials > 0) {
            let hash = x"0134";
            let _ = &hash;
            trials = trials - 1;
        }
    }

    #[test]
    public entry fun test_calibrate_vec_push_back() {
        let trials: u64 = NUM_TRIALS;
        while (trials > 0) {
            let hash = x"0134";
            vector::push_back(&mut hash, 0);
            trials = trials - 1;
        }
    }

    #[test]
    public entry fun test_calibrate_vec_push_back__baseline() {
        let trials: u64 = NUM_TRIALS;
        while (trials > 0) {
            let hash = x"0134";
            let _ = &mut hash;
            let _ = 0u8;
            trials = trials - 1;
        }
    }

    #[test]
    public entry fun test_calibrate_vec_pop_back() {
        let trials: u64 = NUM_TRIALS;
        while (trials > 0) {
            let hash = x"0134";
            vector::push_back(&mut hash, 0);
            trials = trials - 1;
        };
        trials = NUM_TRIALS;
        while (trials > 0) {
            let hash = x"0134";
            vector::pop_back(&mut hash);
            trials = trials - 1;
        }
    }

    #[test]
    public entry fun test_calibrate_vec_pop_back__baseline() {
        let trials: u64 = NUM_TRIALS;
        while (trials > 0) {
            let hash = x"0134";
            vector::push_back(&mut hash, 0);
            trials = trials - 1;
        };
        trials = NUM_TRIALS;
        while (trials > 0) {
            let hash = x"0134";
            let _ = &mut hash;
            trials = trials - 1;
        }
    }

    #[test]
    public entry fun test_calibrate_vec_pack() {
        let trials: u64 = NUM_TRIALS;
        while (trials > 0) {
            let _ = vector [trials];
            trials = trials - 1;
        }
    }
    #[test]
    public entry fun test_calibrate_vec_pack__baseline() {
        let trials: u64 = NUM_TRIALS;
        while (trials > 0) {
            let _ = trials;
            trials = trials - 1;
        }
    }

    #[test]
    public entry fun test_calibrate_vec_swap() {
        let trials: u64 = NUM_TRIALS;
        while (trials > 0) {
            let hash = x"0134";
            vector::swap(&mut hash, 0, 1);
            trials = trials - 1;
        }
    }
    #[test]
    public entry fun test_calibrate_vec_swap__baseline() {
        let trials: u64 = NUM_TRIALS;
        while (trials > 0) {
            let hash = x"0134";
            let _ = &mut hash;
            let _ = 0u64;
            let _ = 1u64;
            trials = trials - 1;
        }
    }

    #[test]
    public entry fun test_calibrate_vec_imm_borrow() {
        let trials: u64 = NUM_TRIALS;
        while (trials > 0) {
            let hash = x"0134";
            vector::borrow(&hash, 0u64);
            trials = trials - 1;
        }
    }
    #[test]
    public entry fun test_calibrate_vec_imm_borrow__baseline() {
        let trials: u64 = NUM_TRIALS;
        while (trials > 0) {
            let hash = x"0134";
            let _ = &hash;
            let _ = 0u64;
            trials = trials - 1;
        }
    }

    #[test]
    public entry fun test_calibrate_vec_mut_borrow() {
        let trials: u64 = NUM_TRIALS;
        while (trials > 0) {
            let hash = x"0134";
            vector::borrow_mut(&mut hash, 0);
            trials = trials - 1;
        }
    }
    #[test]
    public entry fun test_calibrate_vec_mut_borrow__baseline() {
        let trials: u64 = NUM_TRIALS;
        while (trials > 0) {
            let hash = x"0134";
            let _ = &mut hash;
            let _ = 0u64;
            trials = trials - 1;
        }
    }
}

// TODO:
// MoveLoc, StLoc, BrTrue, BrFalse, Branch, Call, CallGeneric, Pop, Ret, MutBorrowFieldGeneric, ImmBorrowFieldGeneric, Abort, Nop

// Not supported for Sui:
// MutBorrowGlobal, MutBorrowGlobalGeneric, ImmBorrowGlobal, ImmBorrowGlobalGeneric, Exists, ExistsGeneric, MoveFrom, MoveFromGeneric, 
// MoveTo, MoveToGeneric

// Not supported in Move yet:
// VecUnpack
