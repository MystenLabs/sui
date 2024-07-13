module Enums::mut_match {

    public fun match_mut(mut_param: &mut u64) {
        match (mut_param) {
            mut_var if (*mut_var > 42) => *mut_var = 42,
            _ => (),
        }
    }
}
