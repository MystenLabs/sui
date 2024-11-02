import { bcs } from "@mysten/sui/bcs";
export function Element() {
    return bcs.struct("Element", ({
        bytes: bcs.vector(bcs.u8())
    }));
}