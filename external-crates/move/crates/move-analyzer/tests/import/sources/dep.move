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
}
