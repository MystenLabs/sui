import { bcs } from "@mysten/sui/bcs";
import * as object from "./object";
export function Bag() {
    return bcs.struct("Bag", ({
        id: object.UID(),
        size: bcs.u64()
    }));
}