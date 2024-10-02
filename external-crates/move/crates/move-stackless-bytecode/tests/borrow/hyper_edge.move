// dep: ../move-stdlib/sources/macros.move
// dep: ../move-stdlib/sources/u64.move
// dep: ../move-stdlib/sources/option.move
// dep: ../move-stdlib/sources/ascii.move
// dep: ../move-stdlib/sources/string.move
// dep: ../move-stdlib/sources/vector.move

module 0x7::Collection {
    public struct Collection<T> has drop {
        items: vector<T>,
        owner: address,
    }

    public fun borrow_mut<T>(c: &mut Collection<T>, i: u64): &mut T {
        vector::borrow_mut(&mut c.items, i)
    }

    public fun make_collection<T>(): Collection<T> {
        Collection {
            items: vector::empty(),
            owner: @0x2,
        }
    }
}

module 0x8::Test {
    use 0x7::Collection;

    public struct Token<phantom T> has drop { value: u64 }

    public fun foo<T>(i: u64) {
        let mut c = Collection::make_collection<Token<T>>();
        let t = Collection::borrow_mut(&mut c, i);
        t.value = 0;
    }
}
