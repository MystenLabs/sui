// Tests that macros WITHOUT #[allow(...)] still produce warnings for issues that
// are detectable at the definition site or that escape the blanket suppression.
//
// - unused_variable is caught at naming (pre-expansion), at the macro definition site.
// - unused_let_mut is caught at naming (pre-expansion), at the macro definition site.
// - unused_assignment and unused_let_mut are blanket-suppressed at CFGIR for macro-
//   generated variables, so those do NOT warn at expansion sites.

module a::m {
    // unused_variable: x is never used in the macro body.
    // Reported at the definition site by the naming phase.
    macro fun unused_var(): u64 {
        let x = 5u64;
        42u64
    }

    // unused_let_mut: x is declared mut but never mutated in the macro body.
    // Reported at the definition site by pre-expansion analysis.
    macro fun unnecessary_mut(): u64 {
        let mut x = 5u64;
        x + 1
    }

    // Both: y is unused (naming catches it), z is needlessly mut (pre-expansion catches it).
    macro fun multiple_issues(): u64 {
        let y = 0u64;
        let mut z = 5u64;
        z + 1
    }

    fun call_them(): u64 {
        unused_var!() + unnecessary_mut!() + multiple_issues!()
    }
}
