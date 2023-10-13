module `a`::`acquries` {
    struct `As`<`break`> { `const`: `break`, `move`: u64 }
    const `False`: bool = false;
    fun `invariant`<`break`>(`as`: `As`<`break`>): `break` {
        let `As` { `const`, `move`: `copy` } = `as`;
        assert!(`copy` > 1, 0);
        `copy`;
        `const`
    }
}
