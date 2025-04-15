// Tests autocompletion with control expressions (`if`, `while` and `match`)
// when these expressions are only partially parse-able
#[allow(ide_path_autocomplete)]
module a::m {

    public struct A has copy, drop {
        x: u64
    }

    public fun foo(p: u32): u32 {
        p
    }

    public fun test_if(a: A) {
        if (a.
    }

    public fun test_while() {
        let n = (42 as u32);
        while (n.
    }

    public fun test_match(n: u64) {
        match (n.
    }

}
