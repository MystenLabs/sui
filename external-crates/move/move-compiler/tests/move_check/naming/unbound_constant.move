address 0x42 {
module M {
    fun t() {
        let x = CONSTANT; x;
        let y = Self::CONSTANT; y;
        0 + CONSTANT + Self::CONSTANT;
    }
}
}

script {
    fun t() {
        let x = CONSTANT; x;
        let y = Self::CONSTANT; y;
        0 + CONSTANT + Self::CONSTANT;
    }
}
