module 0x0::T {
    public enum E {
        V x{: u8},
    }
    fun f(): E {
        E::V { x: 0 }
    }
}
