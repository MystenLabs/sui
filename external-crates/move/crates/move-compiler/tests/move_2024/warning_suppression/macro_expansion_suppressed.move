// Tests that warnings which would arise from macro EXPANSION are suppressed by
// is_from_macro_expansion() in CFGIR, while definition-site issues still warn.
//
// These macros have no #[allow], so definition-site issues (like unused parameters)
// are reported by the naming phase. But their expanded code would produce
// unused_assignment or unused_let_mut warnings at the CFGIR level — the blanket
// suppression silences those because macro-generated variables (color > 0) are
// unconditionally skipped for those checks.

module a::m {
    // $x is unused in the macro body, so naming warns about the unused parameter.
    // After expansion, the by-value binding for $x is an assignment that is never
    // read — unused_assignment would also fire without the blanket suppression.
    macro fun ignore_arg($x: u64): u64 {
        42u64
    }

    // The macro uses `let mut i` and mutates it. But if the lambda causes an early
    // return, the mutation is unreachable and CFGIR would warn about unused_let_mut.
    // The blanket suppression prevents this false positive.
    macro fun for_each($start: u64, $stop: u64, $body: |u64|) {
        let mut i = $start;
        let stop = $stop;
        while (i < stop) {
            $body(i);
            i = i + 1;
        }
    }

    // The macro reassigns y internally. After expansion, the initial assignment
    // `y = $x` is unused because y is immediately overwritten. The blanket
    // suppression handles this.
    macro fun reassign_internal($x: u64): u64 {
        let mut y = $x;
        y = 99u64;
        y
    }

    fun use_them(): u64 {
        ignore_arg!(5u64);
        for_each!(0, 10, |_x| {});
        reassign_internal!(1u64)
    }
}
