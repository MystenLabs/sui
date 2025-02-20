module 0x0::variable_undefined;

const V_1: u8 = 0;
const V_2: u8 = 1;

public fun get_v1(a: u8): u8 {
    match (a) {
        V_1 => 10,
        V_2 => 20,
        _ => abort,
    }
}
