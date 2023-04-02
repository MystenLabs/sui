//# init --addresses A=42



//# run --signers A
script {
    use std::signer;

    fun main(s: signer) {
        assert!(signer::address_of(&s) == @42, 1000);
    }
}



//# publish
module A::M {
    struct Foo has key {
        x: u64,
    }

    public fun publish_foo(s: &signer) {
        move_to<Foo>(s, Foo { x: 500 })
    }
}



//# run --signers A
script {
    use A::M;

    fun main(s: signer) {
        M::publish_foo(&s);
    }
}



// Note: named addresses are not supported in resource names yet.
//# view --address A --resource 0x2a::M::Foo
