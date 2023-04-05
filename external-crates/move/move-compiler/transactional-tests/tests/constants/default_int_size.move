//# run
script {
    // should fail
    // Checks that default integers will always be u64 otherwise existing impls might fail
    // We're going above u64 max
    fun main() {
        let i = 1;
        let j = 1;
        while (j < 65) {
            i = 2 * i;
            j = j + 1;
        };
    }
}

//# run
script {
    // Checks that default integers will always be u64 otherwise existing impls might fail
    fun main() {
        let i = 1;
        let j = 1;
        while (j < 64) {
            i = 2 * i;
            j = j + 1;
        };
    }
}

//# run
script {
    // Checks that default integers will always be u64 otherwise existing impls might fail
    fun main() {
        let i = 256;
        let j = 1;
        while (j < (64 - 8)) {
            i = 2 * i;
            j = j + 1;
        };
    }
}

//# run
script {
    // Checks that default integers will always be u64 otherwise existing impls might fail
    fun main() {
        let i = 65536;
        let j = 1;
        while (j < (64 - 16)) {
            i = 2 * i;
            j = j + 1;
        };
    }
}
