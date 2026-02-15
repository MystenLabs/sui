module 0x42::M {
    public enum Outer {
        X { y: Inner }
    }

    public enum Inner {
        Y(u64),
        Z,
    }

    fun f(o: Outer): u64 {
        match (o) {
            // Malformed: duplicate Outer::X syntax, invalid nested pattern
            Outer::X Outer::X(Inner::Q { v0x0::M }) => 0,
        }
    }
}
