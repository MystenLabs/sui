//# init --edition 2024.alpha

//# publish
module 0x42::X {
    public struct T has drop {}
    public(package) fun new(): T {
        T {}
    }
}

//# publish
module 0x42::Y {
    use 0x42::X;
    public fun foo(): X::T {
        X::new()
    }
}
