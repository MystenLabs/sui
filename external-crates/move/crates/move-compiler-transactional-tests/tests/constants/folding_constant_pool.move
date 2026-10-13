//# init --edition 2024.alpha

//# print-bytecode
module 0x42::m {
    const A: u8 = 97;
    const B: u8 = 98;
    const AB: vector<u8> = b"ab";
    const ADDR: address = @0x1;

    // a folded vector equal to a named constant reuses its pool entry
    public fun reuses(): vector<u8> { vector[A, B] }

    // a folded vector with no equal constant adds a pool entry
    public fun creates(): vector<u8> { vector[B, A] }

    // folding the same new value again reuses that entry
    public fun creates_again(): vector<u8> { vector[B, A] }

    // folded integers are loaded directly
    public fun scalar(): u8 { A + B }

    // folding away an address comparison leaves no load of the address or the constant
    public fun compare(): bool { ADDR == @0x1 }
}
