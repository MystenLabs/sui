module 0x42::a {

    fun t0(): u64 {
        if ('block: { true }) {
            10
        } else {
            20
        }
    }

    fun t1(): u64 {
        if ('block: { true }) {
            'block: { 10 }
        } else {
            20
        }
    }

    fun t2(): u64 {
        if ('block: { false }) {
            20
        } else {
            'block: { 10 }
        }
    }

    fun t3(): u64 {
        if ('block: { false }) {
            20
        } else {
            while (false) { 'block: { 20 }; };
            'block: { 10 }
        }
    }

    fun t4(): u64 {
        let mut count = 0;
        let mut x = 0;
        while (x < 10) {
            'inner: {
                count = count + 1;
            };
            x = x + 1;
        };
        count
    }

}
