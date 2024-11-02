import { bcs } from "@mysten/sui/bcs";
export function BitVector() {
    return bcs.struct("BitVector", ({
        length: bcs.u64(),
        bit_field: bcs.vector(bcs.bool())
    }));
}