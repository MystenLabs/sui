// allowed, a UID from an unpacked object can be used to pack a new object
module a::m {
    use sui::object::UID;
    use sui::transfer::transfer;

    struct Cat has key {
        id: UID,
    }

    struct Dog has key {
        id: UID,
    }

    struct Wrapper {
        id: UID,
    }

    public fun repack(cat: Cat): Cat {
        let Cat { id } = cat;
        Cat { id }
    }

    public fun transmute(cat: Cat) {
        let Cat { id } = cat;
        transfer(Dog { id }, @0);
    }

    public fun through_wrapper(cat: Cat) {
        let Cat { id } = cat;
        let Wrapper { id } = Wrapper { id };
        transfer(Dog { id }, @0);
    }

    fun new_dog(id: UID): Dog {
        Dog { id }
    }

    public fun through_call(cat: Cat) {
        let Cat { id } = cat;
        transfer(new_dog(id), @0);
    }
}

module sui::object {
    struct UID has store {
        id: address,
    }
}

module sui::transfer {
    public fun transfer<T: key>(_: T, _: address) {
        abort 0
    }
}
