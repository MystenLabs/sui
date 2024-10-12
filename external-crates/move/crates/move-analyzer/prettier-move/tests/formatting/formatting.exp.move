// This is a line comment
/*

this is a multi-line block comment

*/

// This is a line comment

module tests::formatting {
    public struct Beep {
        transferred_to_object: VecMap<ID /* owner */, ID>,
    }

    public fun list() {
        let a = vector[
            100, // hahaha
            200, // hihihi
        ];

        let b = vector[100, /* hahaha */ 200 /* hihihi */];

        let c /* comment in between */: vector<u64> = vector[
            100, // hahaha
            200, // hihihi
        ];

        let t /* comment */: vector<u64> = vector<u64 /* hahaha */>[100, 200];
    }
}
