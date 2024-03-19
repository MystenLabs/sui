module a::m {

    public struct S {}

    fun t0(s: &mut S): &S {
        loop {
            break s
        }
    }

    fun t1(s: &mut S): &S {
        'block: {
            s
        }
    }

    #[allow(dead_code)]
    fun t2(s: &mut S): &S {
        'block: {
            return 'block s;
            s
        }
    }

    fun t3(s: &mut S): &S {
        {
            s
        }
    }

    fun t4(s: &mut S): &S {
        {
            s
        }
    }

    fun t5(s: &mut S): &S {
        if (true) { s } else { s }
    }

}
