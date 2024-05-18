module 0x42::m {

    public enum Option<T> has drop {
        None,
        Some(T)
    }

    public struct S has drop {}

    fun check_imm_ref_s(_s: &S): bool {
        false
    }

    fun t0(o: &Option<S>, default: &S): &S {
        match (o) {
            Option::Some(n) if check_imm_ref_s(n) => n,
            Option::Some(y) => y,
            Option::None => default,
        }
    }

    fun check_mut_ref_s(_s: &mut S): bool {
        false
    }

    fun t1(o: &mut Option<S>, default: &mut S): &mut S {
        match (o) {
            Option::Some(n) if check_mut_ref_s(n) => n,
            Option::Some(y) => y,
            Option::None => default,
        }
    }

    fun t2(o: &mut Option<S>, default: &mut S): &S {
        match (o) {
            Option::Some(n) if check_mut_ref_s(n) => n,
            Option::Some(y) => y,
            Option::None => default,
        }
    }

    fun t3(default: &mut S): &S {
        'block: {
            default
        }
    }

    fun t4(default: &mut S): &S {
        loop {
            break default
        }
    }
}
