module 0x42::m {

    public enum ABC<T> has drop {
        A(T),
        B,
        C(T)
    }

    fun with_break(): u64 {
        let items = vector[ABC::C(1u64), ABC::B, ABC::C(3u64)];
        let mut sum = 0u64;
        let mut i = 0;
        loop {
            if (i >= items.length()) break;
            let item = &items[i];
            let ABC::C(x) = item else { i = i + 1; continue };
            sum = sum + *x;
            i = i + 1;
        };
        sum
    }

}
