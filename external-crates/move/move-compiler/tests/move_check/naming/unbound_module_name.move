address 0x42 {
module N {
}
module M {
    use 0x42::N;
    fun t() {
        let x = N::c; x;
        let y = Self::c; y;
        0 + N::c + Self::c;
    }
}
}
