import { bcs } from "@mysten/sui/bcs";
export function FixedPoint32() {
    return bcs.struct("FixedPoint32", ({
        value: bcs.u64()
    }));
}