module Autofix::colon_colon {

    use Autofix::dep;

    public fun single_name_type(s: PubStruct) {
    }

    public fun single_name_fun(): dep::PubStruct {
        let s: dep::PubStruct = create_struct();
        s
    }

    public fun single_name_mod() {
        let s: another_dep;
    }

    // importing the module should be offered here, as importing the member itself
    // would not resolve the unbound module prefix
    public fun two_names(s: another_dep::AnotherDepStruct) {
    }

}
