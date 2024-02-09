module 0x42::M {
    fun foo() {
        (if (true) 5 else 0)();
        (while (false) {})(0, 1);
    }
}
