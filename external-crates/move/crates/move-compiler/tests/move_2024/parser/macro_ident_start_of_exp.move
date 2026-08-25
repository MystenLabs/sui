module 0x8675309::M {
    // A macro identifier expression should be recognized as the start of an
    // expression following `return`, `break`, and `abort`.
    macro fun r($v: u64): u64 {
        if ($v > 0) return $v;
        0
    }

    macro fun b($v: u64): u64 {
        loop {
            if ($v > 0) break $v
        }
    }

    macro fun a($v: u64): u64 {
        if ($v > 0) abort $v;
        0
    }

    fun t() {
        r!(1);
        b!(1);
        a!(1);
    }
}
