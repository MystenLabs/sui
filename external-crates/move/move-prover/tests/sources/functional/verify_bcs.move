// This file is created to verify the native function in the standard BCS module.
module 0x42::VerifyBCS {
    use std::bcs;


    public fun verify_to_bytes<MoveValue>(v: &MoveValue): vector<u8>
    {
        bcs::to_bytes(v)
    }
    spec verify_to_bytes {
        ensures result == bcs::serialize(v);
    }
}
