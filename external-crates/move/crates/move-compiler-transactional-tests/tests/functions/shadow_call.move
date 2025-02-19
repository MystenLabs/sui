//# init --edition 2024.alpha

//# publish
module 0x42::m {
    public fun y() { }
}

//# publish
module 0x43::m{
    use 0x42::m;

    public fun y() {
        m::y();
    }
}

//# run
module 0x44::main {
    use 0x43::m;

    fun main() {
        m::y();
    }
}
