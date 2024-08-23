module a::m {

    public struct T<Q> has drop { q: Q }

    fun any<T>(): T { abort 0 }

    fun globals00(): bool {
        let x: T<u64> = any();
        let y: &T<u64> = abort 0;
        let z: &mut T<u64> = abort 0;
        x == y && x == z
    }

    fun globals01(): bool {
        let x: T<u64> = any();
        let y: &T<bool> = abort 0;
        let z: &mut T<bool> = abort 0;
        x == y && x == z
    }

    fun globals02(): u64 {
        let x = any();
        let y = &any();
        let z = &mut any();
        x == y && x == z;
        (x : u64)
    }

    fun globals03(): u64 {
        let x = any();
        let y = &any();
        let z = &mut any();
        x == y && x == z;
        (x : u64)
    }

    fun locals00(): bool {
        let a: &T<u64> = abort 0;
        let b: &mut T<u64> = abort 0;
        let (c, d): (&mut T<u64>, &T<u64>) = abort 0;
        a == b && c == d && a == c && b == d
    }

    fun locals01(): bool {
        let a: &T<u64> = abort 0;
        let b: &mut T<u64> = abort 0;
        let (c, d): (&mut T<bool>, &T<bool>) = abort 0;
        a == b && c == d && a == c && b == d
    }

    fun locals02(): bool {
        let x = abort 0;
        let y = &(abort 0);
        let z = &mut (abort 0);
        x == y && x == z;
        let _ = (x : u64);
        x == y && x == z
    }

    fun options00(): bool {
        let mut c = option_none();
        c == c;
        let x: T<u64> = option_take(&mut c);
        x == &mut T { q: 5 };
        option_fill(&mut c, T { q: 10 });
        x == x
    }

    fun options01(): bool {
        let mut c = option_none();
        c == c;
        let x: T<u64> = option_take(&mut c);
        x == &mut T { q: false };
        option_fill(&mut c, T { q: false });
        x == x
    }

    fun options02(): T<u64> {
        let mut c = option_none();
        c == c;
        let x = option_take(&mut c);
        x == &mut T { q: 5 };
        option_fill(&mut c, T { q: 10 });
        x == x;
        x
    }

    // Approximation of options for typing

    public struct Option<T> has copy, drop { value: T }

    public fun option_none<Element>(): Option<Element> { abort 0 }

    public fun option_fill<Element>(_t: &mut Option<Element>, _e: Element) { abort 0 }

    public fun option_take<Element>(_t: &mut Option<Element>): Element { abort 0 }

}
