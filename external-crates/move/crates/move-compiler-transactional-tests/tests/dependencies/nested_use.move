//# init --edition 2024.alpha

//# publish
module 0x42::example {
    use std::{vector::{Self as vec, push_back}, string::{String, Self as str}};

    fun example(s: &mut String) {
        let mut v = vec::empty();
        push_back(&mut v, 0);
        push_back(&mut v, 10);
        str::append_utf8(s, v);
    }
}
