module 0x2a::charge_tests;

// All fixed-cost instructions: LdU64 x3, Add x2, Ret
public fun pure_arithmetic(): u64 {
    1 + 2 + 3
}

// Only variable-cost instructions: MoveLoc, Ret
public fun variable_only(x: u64): u64 {
    x
}

// Multiple basic blocks from branching
public fun branching(x: u64): u64 {
    if (x > 10) {
        x + 1
    } else {
        x + 2
    }
}

// Loop creates repeated block execution
public fun looping(mut x: u64): u64 {
    while (x < 100) {
        x = x + 1;
    };
    x
}
