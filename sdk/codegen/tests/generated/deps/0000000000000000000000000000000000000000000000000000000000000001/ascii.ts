import { bcs } from "@mysten/sui/bcs";
export function String() {
    return bcs.struct("String", ({
        bytes: bcs.vector(bcs.u8())
    }));
}
export function Char() {
    return bcs.struct("Char", ({
        byte: bcs.u8()
    }));
}