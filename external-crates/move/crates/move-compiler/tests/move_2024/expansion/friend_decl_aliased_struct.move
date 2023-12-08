address 0x42 {
module M {
    public struct M {}
}

module N {
    use 0x42::M::M;
    friend M;

    public(friend) fun m(_b: M) {}
}
}
