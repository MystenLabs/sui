import { bcs } from "@mysten/sui/bcs";
export function BCS() {
    return bcs.struct("BCS", ({
        bytes: bcs.vector(bcs.u8())
    }));
}