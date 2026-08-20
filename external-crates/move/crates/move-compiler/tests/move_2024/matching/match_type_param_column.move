// Tests that a type-parameter-typed field may precede a discriminating field on the match
// fringe (GH #26788). The `asset: T` column has no structure to inspect, so match
// compilation must bind it and move on to `state`.
module 0x42::m {

    public enum State has drop {
        On,
        Off,
    }

    public struct Inner<T> has drop { asset: T, state: State }

    public struct Outer<T> has drop { inner: Inner<T> }

    public fun by_value<T>(o: Outer<T>): T {
        match (o) {
            Outer { inner: Inner { asset, state: State::On } } => asset,
            Outer { inner: Inner { asset, state: State::Off } } => asset,
        }
    }

    public fun by_ref<T>(o: &Outer<T>): bool {
        match (o) {
            Outer { inner: Inner { asset: _, state: State::On } } => true,
            Outer { inner: Inner { asset: _, state: State::Off } } => false,
        }
    }

    public fun by_mut_ref<T>(o: &mut Outer<T>): bool {
        match (o) {
            Outer { inner: Inner { asset: _, state: State::On } } => true,
            Outer { inner: Inner { asset: _, state: State::Off } } => false,
        }
    }
}
