module 0x42::m;

public enum Maybe<T> {
    Just(T),
    Nothing
}

fun test(z: &mut Maybe<u64>) {
    let { match (z) { Maybe::Just(n) => n, Maybe::Nothing => abort 0 } } = 5;
}


