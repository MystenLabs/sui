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

    /* prettier-ignore */
    fun block_comment_form(  a:u8  ) {   }

    /// A doc comment that merely mentions `prettier-ignore` must not
    /// disable formatting of the item below it.
    fun formatted_normally(a: u8, b: u8) {}
}

// prettier-ignore
module prettier::another {
    }
