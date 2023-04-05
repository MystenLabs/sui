address 0x42 {
module M {
    // invalid duplicate abilities
    struct Foo has copy, copy {}
    struct Bar<T: drop + drop> { f: T }
    fun baz<T: store + store>() {}

    spec module {
        invariant<T: key + key> exists<T>(0x1) == exists<T>(0x1);
        axiom<T: store + store + key + key> exists<T>(0x2);
    }
}
}
script {
    fun main<T: key + key>() {}
}
