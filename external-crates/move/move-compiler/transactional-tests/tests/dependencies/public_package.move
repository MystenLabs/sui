//#init --edition 2024.alpha

//# publish
address 0x42 {
    module X {
        struct T has drop {}

        public(package) fun new(): T {
            T {}
        }
    }

    module Y {
        use 0x42::X;

        public fun foo(): X::T {
            X::new()
        }
    }
}


//# run
script {
use 0x42::Y;

fun main() {
    Y::foo();
}
}
