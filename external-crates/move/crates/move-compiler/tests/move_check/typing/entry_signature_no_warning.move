module 0x42::M {

    struct CoolStruct has drop {}

    // entry functions no longer have any built in checks outside of visibility rules

    public entry fun signer_ref(_: &signer) {}

    public entry fun late_signer(_u: u64, _s: signer) {}

    public entry fun struct_arg(_: CoolStruct) {}

    public entry fun u64_ret(): u64 {
        0
    }

    public entry fun struct_ret(): CoolStruct {
        CoolStruct {}
    }

}
