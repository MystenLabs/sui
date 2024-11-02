import { bcs } from "@mysten/sui/bcs";
export function TypeName() {
    return bcs.struct("TypeName", ({
        name: bcs.string()
    }));
}