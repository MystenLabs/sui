module test::functions {
    /// Comment for a function.
    fun empty() {
    }

    public fun pub() {
    }

    public entry fun pub_entry() {
    }

    entry public fun entry_pub() {
    }

    fun foo(
        foo: u64,
        bar: u64,
        baz: u64,
        foo: u64,
    ): u64 {
        foo + bar + baz
    }

    fun foo(
        foo: u64,
        bar: u64,
        baz: u64,
        foo: u64,
        bar: u64,
        baz: u64,
        foo: u64,
        bar: u64,
        baz: u64,
    ): u64 {
        foo + bar + baz
    }

    fun foo(
        // first
        foo: u64,
        // second
        bar: u64,
        baz: u64,
    ): u64 {
        foo + bar + baz
    }

    fun foo(
        foo: u64, // first
        bar: u64, // second
        baz: u64,
    ): u64 {
        foo + bar + baz
    }


    fun simple(p: u64): u64 {
    }

    fun simple_generic<T1  :    key,
       T2
    :
            store
       + drop + key
    ,
    >(
        )
    {
    }

    fun long_type_list(p1: SomeStructWithALongName,
               p2: SomeStructWithALongName, p3:


        SomeStructWithALongName, p1: SomeStructWithALongName): u64 {
    }

    fun long_type_list_and_generics<TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT1  :    key, TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT2  :    store, TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT3  :    drop>

    (p1: SomeStructWithALongName,
               p2: SomeStructWithALongName, p3:


        SomeStructWithALongName, p1: SomeStructWithALongName): u64 {
    }

    fun long_type_list_generics_and_body<TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT1  :    key, TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT2  :    store, TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT3  :    drop>

    (p1: SomeStructWithALongName,
               p2: SomeStructWithALongName, p3:


        SomeStructWithALongName, p1: SomeStructWithALongName): u64 {
            some_long_function_name();
                              some_long_function_name();
            some_long_function_name();  some_long_function_name();
   some_long_function_name();
                             some_long_function_name();
            some_long_function_name();
    }

    native fun simple_native(p: u64): u64;

    public native fun  public_native(p: u64): u64;

    native fun simple_native_generic<T1  :    key,
       T2
    :
            store
       + drop + key
    ,
    >(
        ): u64;


    // Comment for a function.
    fun with_comment(p: u64): u64 {}
}
