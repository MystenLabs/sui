import { bcs } from "@mysten/sui/bcs";
import * as object from "./object";
export function Publisher() {
    return bcs.struct("Publisher", ({
        id: object.UID(),
        package: bcs.string(),
        module_name: bcs.string()
    }));
}
export function UpgradeCap() {
    return bcs.struct("UpgradeCap", ({
        id: object.UID(),
        package: bcs.Address,
        version: bcs.u64(),
        policy: bcs.u8()
    }));
}
export function UpgradeTicket() {
    return bcs.struct("UpgradeTicket", ({
        cap: bcs.Address,
        package: bcs.Address,
        policy: bcs.u8(),
        digest: bcs.vector(bcs.u8())
    }));
}
export function UpgradeReceipt() {
    return bcs.struct("UpgradeReceipt", ({
        cap: bcs.Address,
        package: bcs.Address
    }));
}