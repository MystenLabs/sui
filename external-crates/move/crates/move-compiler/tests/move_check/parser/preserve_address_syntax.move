// make sure addresses are printed as parsed
// but zeros are still trimmed
module 0x42::m {
    fun ex() {
        0x00042::M::foo();
        000112::N::bar();
    }
}
