// tests for cases that self-assignment could warn, but currently dont

module a::m {
    fun t(cond: bool, other: u64) {
        let x = 0;
        x = if (cond) x else x;
        x;

        x = if (cond) x else other;
        x;

        x = { 0; x };
        x;

        x = { let y = 0; y; x };
        x;

        // TODO move most lints to 2024
        // x = match (cond) { true => x, false => x };
        // x;

        let x = &other;
        other = *x;
        other;
    }
}
