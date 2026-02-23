module 0x0::M {
    public enum E has drop { W(bool) }
    fun f(e: E): u64 {
        match (e) { E::W(0) => 1 }
    }
}
