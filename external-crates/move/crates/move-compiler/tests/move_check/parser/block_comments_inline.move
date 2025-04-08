module 0x42::m {

    struct S {
        /*comment*/ w: u64,
        /*comment*/ x: u64,
        /**/ y: u64,
        /** doc */ z: u64,
    }

    fun t(w: u64, x: u64, /*ignore*/y: u64, z:u64/**/): (u64, u64, u64, u64) {
        let /*comment*/s = S { /***/w, /**/x, /*comment*/y, /** doc */z };
        let S { /****/w, /**/x, /*comment*/y, /** doc */z } = /*comment*/s;
        (w, x, y, z)
    }
}
