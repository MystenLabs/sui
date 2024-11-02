import { bcs } from "@mysten/sui/bcs";
import * as object from "./object";
export function Clock() {
    return bcs.struct("Clock", ({
        id: object.UID(),
        timestamp_ms: bcs.u64()
    }));
}