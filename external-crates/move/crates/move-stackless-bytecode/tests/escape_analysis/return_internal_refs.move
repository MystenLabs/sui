module 0x1::LeakInternalRefs {

    struct S { f: u64, g: u64 }

    fun leak_mut_ref(s: &mut S): &mut u64 {
        &mut s.f
    }

    fun leak_immut_ref(s: &S): &u64 {
        &s.f
    }

    fun leak_two_refs(s: &mut S): (&mut u64, &mut u64) {
        (&mut s.f, &mut s.g)
    }

    fun leak_in_branch(b: bool, x: &mut u64, s: &mut S): &mut u64 {
        if (b) {
            x
        } else {
            &mut s.f
        }
    }

    fun leak_in_loop(x: &mut u64, s: &mut S): &mut u64 {
        let i = 0;
        while (i < 10) {
            if (i == 7) {
                return &mut s.f
            };
            i = i + 1;
        };
        x
    }

    fun read_but_dont_leak(x: &mut u64, s: &mut S): &mut u64 {
        let _ = &mut s.f;
        x
    }

}
