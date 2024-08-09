// :!:>script
script {
    use test_account::cli_args;
    use std::vector;

    /// Get a `bool` vector where each element indicates `true` if the
    /// corresponding element in `u8_vec` is greater than `u8_solo`.
    /// Then pack `address_solo` in a `vector<vector<<address>>` and
    /// pass resulting argument set to public entry function.
    fun set_vals<T1, T2>(
        account: signer,
        u8_solo: u8,
        u8_vec: vector<u8>,
        address_solo: address,
    ) {
        let bool_vec = vector::map_ref(&u8_vec, |e_ref| *e_ref > u8_solo);
        let addr_vec_vec = vector[vector[address_solo]];
        cli_args::set_vals<T1, T2>(account, u8_solo, bool_vec, addr_vec_vec);
    }
} // <:!:script
