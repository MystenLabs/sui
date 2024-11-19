// Test various "use" declarations at the file level.
module 0x42::m {
    use 0x0::Module;
    use 0x0000::Z;
    use 0xaBcD::Module as M;

    fun main() {}
}
