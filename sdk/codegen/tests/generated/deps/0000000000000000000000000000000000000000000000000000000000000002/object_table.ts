import { bcs } from "@mysten/sui/bcs";
import * as object from "./object";
export function ObjectTable() {
    return bcs.struct("ObjectTable", ({
        id: object.UID(),
        size: bcs.u64()
    }));
}