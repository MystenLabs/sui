module 0x42::m {

    public enum ABC<T> has drop {
        A(T),
        B,
        C(T)
    }

    fun t0(x: ABC<u64>): u64 {
        let default = 1;
        let y = match (&x) {
            ABC::C(x) => x,
            ABC::A(x) => x,
            ABC::B => &default,
        };
        let y = *y;
        let z = match (x) {
            ABC::C(x) => y + x,
            ABC::A(x) => y + x,
            ABC::B => y,
        };
        z
    }

    fun t1(x: &ABC<u64>): u64 {
        let default = 1;
        let y = match (x) {
            ABC::C(x) => x,
            ABC::A(x) => x,
            ABC::B => &default,
        };
        let y = *y;
        let z = match (x) {
            ABC::C(x) => y + *x,
            ABC::A(x) => y + *x,
            ABC::B => y,
        };
        z
    }

    fun t2(x: &mut ABC<u64>): u64 {
        let mut default = 1;
        let y = match (x) {
            ABC::C(x) => x,
            ABC::A(x) => x,
            ABC::B => &mut default,
        };
        let y = *y;
        let z = match (x) {
            ABC::C(x) => y + *x,
            ABC::A(x) => y + *x,
            ABC::B => y,
        };
        z
    }


}
