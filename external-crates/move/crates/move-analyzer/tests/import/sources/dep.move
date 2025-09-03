module Import::dep {
    const DEP_CONST: u32 = 42;

    public struct PubStruct {
    }

    public enum PubEnum {
        SomeVariant
    }

    public fun pub_fun() {
    }

    public(package) fun pkg_fun() {
    }

    fun private_fun() {
    }

    // test insertion without imports
    public fun bar() {
        d                         // nothing from colon_colon module should be on the auto-imports list
    }
}
