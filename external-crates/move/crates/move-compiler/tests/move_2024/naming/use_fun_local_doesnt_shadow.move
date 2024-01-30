module a::m {
    fun id(x: u64): u64 { x }

    macro fun apply($id: |u8| -> u8) {
        use fun id as u64.id; // not shadowed by the local
        $id((0u64.id() as u8));
    }

    fun t() {
        apply!(|x| x);
    }
}
