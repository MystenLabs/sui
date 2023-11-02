// return values from entry functions must have drop

module a::m {
    public entry fun t0(): u64 {
        abort 0
    }

    public entry fun t1(): (u64, u8) {
        abort 0
    }

    public entry fun t2(): vector<u8> {
        abort 0
    }

    struct Droppable has drop { flag: bool }
    public entry fun t3(): Droppable {
        abort 0
    }

    public entry fun t4(): (vector<Droppable>, u64) {
        abort 0
    }
}
