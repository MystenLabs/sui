module a::m;

public fun foo() {
    a::n::bar();
}

module a::n {

    public fun bar() { }

}

