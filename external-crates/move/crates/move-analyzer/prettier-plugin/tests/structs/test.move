module test::structs {

    struct Empty
    {

    }

    struct Simple
    {
        f
      :
          u64
    }

    struct WithAbilities has
         key
     ,
       drop

    {
        f:   u64,
    }

    struct TwoField {f1     :      u64,
        f2
      :
          u64
    }

    struct SimpleGeneric<T1  :    key,
       T2
    :
            store
       + drop + key
    ,
    >
    {
    }

    struct SimpleGenericWithAbilities
                <T1  :    key,
       T2
    :
            store
       + drop
    ,
    >
 has
         key
    {
    }

    struct OneLongGeneric<TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT  :    key>
    {
    }

    struct ThreeLongGenerics<TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT1  :    key, TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT2  :    store, TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT3  :    drop>
    {
    }

    struct ThreeLongGenericsWithAbilitiesAndFields<TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT1  :    key, TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT2  :    store, TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT3  :    drop>
 has
         key
     ,
      drop
    {
        f1     :      u64,
        f2
      :
          u64
    }

    native struct NativeShort<T
       :  key
    > has
           key;

native struct NativeGenericWithAbilities
                <T1  :    key,
       T2
    :
            store
       + drop + key
    , T3
    >
 has
         key
     ,
      drop;
}
