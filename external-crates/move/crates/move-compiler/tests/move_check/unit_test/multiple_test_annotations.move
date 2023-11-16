// cannot have multiple #[test] attributes. Only one test attribute is allowed,
// and all address arguments need to be assigned in that attribute.
address 0x1 {
module M {
    #[test(_a=@0x1)]
    #[test(_b=@0x2)]
    public fun a(_a: address, _b: address) { }

    #[test]
    #[test(_a=@0x1, _b=@0x2)]
    public fun b(_a: address, _b: address) { }
}
}
