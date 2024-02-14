module a::m {
    // invalid cycle through by-name arguments
    macro fun foo($f: u64): u64 {
        bar!(foo!($f))
    }

    macro fun bar($f: u64): u64 {
        foo!(bar!($f))
    }

    macro fun arg($f: u64): u64 {
        $f + arg!($f)
    }

    macro fun arg_eta($f: u64): u64 {
        $f + arg_eta!({ $f })
    }

    macro fun arg_apply($f: u64): u64 {
        $f + apply!({ arg_apply!($f) })
    }

    macro fun apply($f: u64): u64 {
        $f
    }

    fun t() {
        foo!(0);
        arg!({ let x = 0; x });
        arg_eta!({ let x = 0; x });
        arg_apply!({ let x = 0; x });
    }
}
