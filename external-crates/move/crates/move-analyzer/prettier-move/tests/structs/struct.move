module test::structs {

    struct Empty
    {

    }

    public struct Public
    {

    }

    struct Simple
    {
        f
      :
          u64
    }

    public struct WithAbilities has
         key
     ,
       drop

    {
        f:   u64,
    }

    public struct WithPostfixAbilities
        {
        f:   u64,
      }
     has
         key
     ,
       drop;



    public struct TwoField {f1     :      u64,
        f2
      :
          u64
    }

    public struct SimpleGeneric<T1  :    key,
       T2
    :
            store
       + drop + key
    ,
    >
    {
    }

    public struct SimpleGenericWithAbilities
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

    public struct OneLongGeneric<TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT  :    key>
    {
    }

    public struct ThreeLongGenerics<phantom TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT1  :    key, TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT2  :    store, TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT3  :    drop>
    {
    }

    public struct ThreeLongGenericsWithAbilitiesAndFields<TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT1  :    key, TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT2  :    store, TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT3  :    drop>
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

    public native struct NativeShort<T
       :  key
    > has
           key;

public native struct NativeGenericWithAbilities
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

    public struct PositionalEmpty
    (

        )

    public struct PositionalFields
    (    Empty,
    u64
           )


    public struct PositionalFieldsWithAbilities
    (    Empty,
    u64
           ) has key, store;


    public struct PositionalFieldsLong
    (    PositionalFieldsWithAbilities,
    PositionalFieldsWithAbilities, PositionalFieldsWithAbilities, PositionalFieldsWithAbilities, PositionalFieldsWithAbilities, PositionalFieldsWithAbilities, PositionalFieldsWithAbilities
           )


}
