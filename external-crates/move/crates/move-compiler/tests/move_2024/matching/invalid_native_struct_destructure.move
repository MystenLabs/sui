module 0x0::M {
    native struct S;
    
    fun f(s: S): u64 {
        match (s) {
            S {} => 0,
        }
    }
}
