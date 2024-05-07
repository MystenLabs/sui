module 0x42::m {

    struct X { n : u64 }

    struct Y<X> { x : X }

    struct Z<X> { x : Y<X> }

}
