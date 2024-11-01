module a::m {

    public fun test1() {
        use a::m2::
        let _tmp = 42; // reset parser to see if the next line compiles
        m2::foo();
    }

    public fun test2() {
        use a::m2::{foo
        let _tmp = 42; // reset parser to see if the next line compiles
        foo();
    }

    public fun test3() {
        use a::m2::{foo, bar
        let _tmp = 42; // reset parser to see if the next lines compile
        foo();
        bar();
    }

    public fun test4() {
        use a::{m2::{foo, bar
        let _tmp = 42; // reset parser to see if the next lines compile
        foo();
        bar();

    }

    public fun test5() {
        use a::{m2::{foo, bar}, m3
        let _tmp = 42; // reset parser to see if the next lines compile
        m3::baz();
        foo();
        bar();
    }
}

module a::m2 {

    public fun foo() {}

    public fun bar() {}
}

module a::m3 {

    public fun baz() {}
}
