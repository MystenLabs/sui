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
module sui::BytecodeCalibrationTests {

    // Number of times to run the inner loop of tests
    // We set this value to 1 to avoid long running tests
    // But normally we want something like 1000000
    const NUM_TRIALS: u64 = 1;
    const U64_MAX: u64 = 18446744073709551615;

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

    // TBD
    // Lt, Gt, Le, Ge, Not
}