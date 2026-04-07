module 0x1::inline_test;

fun add(a: u64, b: u64): u64 {
    a + b
}

fun double(x: u64): u64 {
    x + x
}

fun add3(a: u64, b: u64, c: u64): u64 {
    a + b + c
}

fun get_constant(): u64 {
    42
}

public fun caller(): u64 {
    let x = 10;
    let y = 20;
    add(x, y)
}

public fun multi_caller(): u64 {
    let a = add(1, 2);
    let b = add(3, 4);
    add(a, b)
}

public fun double_caller(): u64 {
    double(21)
}

public fun add3_caller(): u64 {
    add3(1, 2, 3)
}

public fun inline_caller(): u64 {
    get_constant()
}

public fun multi_inline_caller(): u64 {
    let a = get_constant();
    let b = get_constant();
    a + b
}

public fun inline_in_conditional(flag: bool): u64 {
    if (flag) {
        get_constant()  // This call will be inlined, inside a branch
    } else {
        100
    }
}

public fun branch_over_inline(flag: bool): u64 {
    let result = if (flag) {
        // This branch jumps over the else block which contains the inlined call
        50
    } else {
        get_constant()  // Inlined call - code expands here
    };
    // Code after the conditional - branch targets here need adjustment
    result + 1
}

public fun complex_branches(a: bool, b: bool): u64 {
    let x = if (a) {
        get_constant()  // First inlined call
    } else {
        0
    };
    let y = if (b) {
        get_constant()  // Second inlined call
    } else {
        1
    };
    x + y
}

// ============================================================================
// Non-integral parameter type tests
// ============================================================================

fun negate(b: bool): bool {
    !b
}

fun bool_and(a: bool, b: bool): bool {
    a && b
}

fun is_zero_addr(addr: address): bool {
    addr == @0x0
}

fun check_value(addr: address, expected: u64): bool {
    addr != @0x0 && expected > 0
}

public fun negate_caller(): bool {
    negate(true)
}

public fun bool_and_caller(): bool {
    bool_and(true, false)
}

public fun is_zero_addr_caller(): bool {
    is_zero_addr(@0x1)
}

public fun check_value_caller(): bool {
    check_value(@0x42, 100)
}

// ============================================================================
// Stack/locals expansion test
// ============================================================================

// This function has parameters and should be inlined
fun inlineable_with_params(a: u64, b: u64): u64 {
    a + b
}

// This caller has NO local variables of its own.
// When inlineable_with_params is inlined, we need temporary locals
// to store the parameters (a, b). If the caller's locals aren't expanded,
// StLoc will fail with an out-of-bounds error.
public fun caller_without_locals(): u64 {
    // No local variables declared here
    // Just call the function directly with literals
    inlineable_with_params(10, 20)
}
