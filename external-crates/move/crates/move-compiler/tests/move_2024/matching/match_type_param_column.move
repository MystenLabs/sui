// Test matching on nested type param fields
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
