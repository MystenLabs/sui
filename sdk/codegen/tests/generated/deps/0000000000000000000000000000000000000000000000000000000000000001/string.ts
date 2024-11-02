import { bcs } from "@mysten/sui/bcs";
export function String() {
    return bcs.struct("String", ({
        bytes: bcs.vector(bcs.u8())
    }));
}