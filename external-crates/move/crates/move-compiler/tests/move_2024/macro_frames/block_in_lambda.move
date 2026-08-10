// Tests that a user-written block inside a lambda body stays in the
// Lambda frame. The block introduces no expansion of its own, so its
// statements must remain attributed to the enclosing Lambda frame:
// without this, the block's statements and the else-branch `0` would
// reset to the user frame mid-lambda, causing spurious pops/pushes.
module A::m {
    macro fun apply($f: |u64| -> u64): u64 {
        $f(1)
    }

    public fun test(p: u64): u64 {
        apply!(|x| {
            let y = x + p;
            if (y > 1) { y } else { 0 }
        })
    }
}
