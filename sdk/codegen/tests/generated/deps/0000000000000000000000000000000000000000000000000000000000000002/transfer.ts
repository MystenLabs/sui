import { bcs } from "@mysten/sui/bcs";
export function Receiving() {
    return bcs.struct("Receiving", ({
        id: bcs.Address,
        version: bcs.u64()
    }));
}