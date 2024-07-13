//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum Option<T> {
        Some(T),
        None
    }

    public fun or_default<T: drop>(opt: Option<T>, default: T): T {
        match (opt) {
            Option::Some(x) => x,
            Option::None => default,
        }
    }

    public fun run() {
        let x = Option::Some(42);
        let y = Option::None;
        assert!(x.or_default(0) == 42);
        assert!(y.or_default(0) == 0);
    }
}

//# run 0x42::m::run
