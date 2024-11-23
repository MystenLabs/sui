// options:
// printWidth: 80

module prettier::prettier_ignore {
    fun ignored_vec() {
        // prettier-ignore
        let a = vector[
            1, 2, 3,
            4, 5, 6,
            7, 8, 9,
        ];
    }
}

// prettier-ignore
module prettier::another {
    }
