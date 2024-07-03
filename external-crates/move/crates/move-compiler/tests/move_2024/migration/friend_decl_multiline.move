module a::m {
    #[ext(
        some_thing
        )
    ]
    friend a::b;

    #[ext(
        q =
            10,
        b
        )
    ]
    friend a::c;
}

module a::b {}
module a::c {}
