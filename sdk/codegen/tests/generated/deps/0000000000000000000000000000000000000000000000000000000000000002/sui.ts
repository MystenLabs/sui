import { bcs } from "@mysten/sui/bcs";
export function SUI() {
    return bcs.struct("SUI", ({
        dummy_field: bcs.bool()
    }));
}