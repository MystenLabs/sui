import { bcs } from "@mysten/sui/bcs";
import * as bag from "./bag";
export function Extension() {
    return bcs.struct("Extension", ({
        storage: bag.Bag(),
        permissions: bcs.u128(),
        is_enabled: bcs.bool()
    }));
}
export function ExtensionKey() {
    return bcs.struct("ExtensionKey", ({
        dummy_field: bcs.bool()
    }));
}