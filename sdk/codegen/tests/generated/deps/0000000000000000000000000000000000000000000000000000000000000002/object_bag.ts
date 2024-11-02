import { bcs } from "@mysten/sui/bcs";
import * as object from "./object";
export function ObjectBag() {
    return bcs.struct("ObjectBag", ({
        id: object.UID(),
        size: bcs.u64()
    }));
}