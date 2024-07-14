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
            // hahaha
            100,
            // hihihi
            200,
        ];

        let c /* comment in between */: vector<u64> = vector[
            // hahaha
            100,
            // hihihi
            200,
        ];
    }
}
