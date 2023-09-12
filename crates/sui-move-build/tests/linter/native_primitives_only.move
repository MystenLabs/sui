// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::test1 {

    struct Coolstruct has copy ,drop {
        a: bool,
        b: u64,
    }

    #[allow(unused_function)]
    native fun returns_something(a:bool,b:u64,c:Coolstruct,d:&Coolstruct) : (bool,u64);

    public entry fun main(){
        let (_x,_y) = returns_something(true,42,Coolstruct{a:true,b:42},&Coolstruct{a:true,b:42});
    }
}

module 0x42::test2 {
    #[allow(unused_function)]
    native fun should_not_complain();
}

module 0x42::test3 {
    #[allow(unused_function)]
    native fun compare_numbers(a:u64,b:u64) : bool;
}
