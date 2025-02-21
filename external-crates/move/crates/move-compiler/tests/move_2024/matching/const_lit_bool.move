module 0x0::variable_undefined;

const V_1: bool = true;
const V_2: bool = false;

public fun get_v1(a: bool): bool {
    match (a) {
        V_1 => true,
        V_2 => false,
        _ => abort,
    }
}
