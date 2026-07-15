// Tests frame attribution when local elimination moves a value across
// an expansion boundary.
//
// `let v = 1` lives in MacroBody(m). The lambda invocation `$f(v)` passes
// `v` across a scope boundary, and the optimizer removes `v` by
// substituting Value(1) at its use site inside the lambda. The
// substituted constant must remain attributed to MacroBody(m), keeping
// it visible as a separate frame transition.
module A::m {
    macro fun m($f: |u64| -> u64): u64 {
        let v = 1;
        $f(v)
    }

    public fun test(): u64 {
        m!(|x| x)
    }
}
