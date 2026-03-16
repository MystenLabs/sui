// Tests that passing an `&mut` reference to a macro, where the macro's lambda
// only uses it immutably, does not produce spurious `unused_let_mut` warnings.
//
// This is a key usability case: a macro may accept `&mut` to be general-purpose,
// but a particular caller's lambda only reads through the reference. The user should
// not be forced to write two versions of the macro just to avoid warnings.

module a::m {
    public struct S has copy, drop { value: u64 }

    // Macro takes a mutable reference and passes it to a lambda.
    // The lambda receives &mut but the caller may only read through it.
    macro fun with_mut_ref($s: &mut S, $f: |&mut S| -> u64): u64 {
        $f($s)
    }

    // Caller's lambda only reads — no mutation through the &mut ref.
    // This should not produce any unused_let_mut or related warnings.
    fun read_only(): u64 {
        let mut s = S { value: 42 };
        with_mut_ref!(&mut s, |r| r.value)
    }

    // Caller's lambda does mutate — also no warnings.
    fun does_mutate(): u64 {
        let mut s = S { value: 0 };
        with_mut_ref!(&mut s, |r| {
            r.value = 99;
            r.value
        })
    }
}
