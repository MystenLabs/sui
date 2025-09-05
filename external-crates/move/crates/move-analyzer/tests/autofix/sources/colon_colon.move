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

    // only qualifcation should be offered here as importing `Autofix::another_dep::AnotherDepEnum`
    // does not actually solve the problem
    public fun two_names(s: another_dep::AnotherDepStruct) {
    }

}
