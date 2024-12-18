#[allow(unused_field)]
module 0x6::M1 {
    use 0x6::M2::C;

    public struct A<T> { f: u64, v: vector<u8>, b: B<T> }

    public struct B<T> { a: address, c: C<T>, t: T }

    public struct S<T> { t: T }

    public struct G { x: u64, s: S<bool> }
}
