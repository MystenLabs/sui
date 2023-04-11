// make sure addresses are printed as parsed
// but zeros are still trimmed
script {
    fun ex() {
        0x00042::M::foo();
        000112::N::bar();
    }
}
