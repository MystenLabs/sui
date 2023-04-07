// Tests the additional axiom that constrains address serialization to have the same size.
module 0x42::AddressSerialization {
    use std::bcs;

    /// Serialized representation of address typed Move values have the same vector length.
    public fun serialized_addresses_same_len(addr1: &address, addr2: &address): (vector<u8>, vector<u8>) {
        (bcs::to_bytes(addr1), bcs::to_bytes(addr2))
    }
    spec serialized_addresses_same_len {
        ensures len(bcs::serialize(addr1)) == len(bcs::serialize(addr2));
        ensures len(result_1) == len(result_2);
    }

    /// Serialized representation of Move values do not have the same length in general.
    public fun serialized_move_values_diff_len_incorrect<MoveValue>(mv1: &MoveValue, mv2: &MoveValue): (vector<u8>, vector<u8>) {
        (bcs::to_bytes(mv1), bcs::to_bytes(mv2))
    }
    spec serialized_move_values_diff_len_incorrect {
        ensures len(bcs::serialize(mv1)) == len(bcs::serialize(mv2));
        ensures len(result_1) == len(result_2);
    }

}
