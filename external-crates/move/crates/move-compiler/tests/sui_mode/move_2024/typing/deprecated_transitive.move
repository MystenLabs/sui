module a::m;

#[deprecated]
public struct A1(u64) has drop;

#[deprecated]
public struct A2(A1) has drop;

#[deprecated]
public fun bad_1(): A1 { let A1(x) = A1(0); A1(x) }
                 // ^ Should not warn about deprecated type in deprecated function

#[deprecated]
                 // v Should not warn about deprecated type in deprecated function
public fun bad_2(): A1 { bad_1() }
                       // ^ Should not warn about deprecated call in deprecated function
