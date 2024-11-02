import { bcs } from "@mysten/sui/bcs";
import * as object from "./object";
export function Table() {
    return bcs.struct("Table", ({
        id: object.UID(),
        size: bcs.u64()
    }));
}