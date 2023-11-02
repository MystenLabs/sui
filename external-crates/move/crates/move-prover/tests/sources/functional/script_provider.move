// A module providing functionality to the script*.move tests
address 0x1 {
module ScriptProvider {
    use std::signer;

    struct Info<phantom T> has key {}

    public fun register<T: store>(account: &signer) {
        assert!(signer::address_of(account) == @0x1, 1);
        move_to(account, Info<T>{})
    }
    spec schema RegisterConditions<T> {
        account: signer;
        aborts_if signer::address_of(account) != @0x1;
        aborts_if exists<Info<T>>(@0x1);
        ensures exists<Info<T>>(@0x1);
    }
    spec register {
        include RegisterConditions<T>;
    }
}
}
