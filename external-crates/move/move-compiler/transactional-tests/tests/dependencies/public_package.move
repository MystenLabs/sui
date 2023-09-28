//# init --edition 2024.alpha

//# publish
address 0x42 {
    module x {
        public struct T has drop {}

        public(package) fun new(): T {
            T {}
        }
    }

    module y {
        use 0x42::x;

        public fun foo(): x::T {
            x::new()
        }
    }
}

//# run 0x42::y::foo
