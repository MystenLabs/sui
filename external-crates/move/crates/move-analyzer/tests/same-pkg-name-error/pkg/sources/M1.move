#[test_only]
module Pkg::M1 {
    public fun foo() {}
}


module Pkg::M2 {

    #[test]
    public fun bar() {
        Pkg::M1::foo();
        DepPkg::M1::foo()
    }

}
