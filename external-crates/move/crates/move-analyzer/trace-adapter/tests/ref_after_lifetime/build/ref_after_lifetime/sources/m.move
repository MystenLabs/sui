// Test that demonstrates the bug where displaying a reference parameter
// fails when the local it points to has ended its lifetime.
module ref_after_lifetime::m;

// Returns the reference parameter without dereferencing it.
// The parameter stays alive in the frame, but the target local's lifetime ends.
fun return_ref_param(value_ref: &u64): &u64 {
    value_ref
}

#[test]
fun test() {
    let my_value = 42u64;

    // This is the LAST time my_value is accessed
    // After this call, my_value's lifetime ends
    let _returned_ref = return_ref_param(&my_value);

    // When stepping into return_ref_param and displaying its parameters:
    // - The parameter (Local [10,0]) is alive and contains a reference
    // - The reference points to my_value (Local [0,0])
    // - But my_value's lifetime has ended (it's undefined)
    // - Attempting to resolve the reference triggers the bug
}
