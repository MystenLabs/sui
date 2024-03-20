module A::m {
    /* #[test_only] */
    /* friend A::b; */

    /* friend A::c; */

    /* #[ext(
        some_thing
        )
    ] */
    /* friend A::d; */

    /* #[ext(
        q =
            10,
        b
        )
    ] */
    /* friend A::e; */
}

module A::a {}
module A::b {}
module A::c {}
module A::d {}
module A::e {}
