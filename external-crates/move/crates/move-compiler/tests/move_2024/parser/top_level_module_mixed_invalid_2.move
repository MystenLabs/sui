module a::n {

    public fun bar() { }

}

module a::m;

public fun foo() {
    a::n::bar();
}


