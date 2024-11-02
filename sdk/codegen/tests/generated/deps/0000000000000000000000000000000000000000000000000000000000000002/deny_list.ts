import { bcs } from "@mysten/sui/bcs";
import * as object from "./object";
import * as bag from "./bag";
import * as table from "./table";
export function DenyList() {
    return bcs.struct("DenyList", ({
        id: object.UID(),
        lists: bag.Bag()
    }));
}
export function ConfigWriteCap() {
    return bcs.struct("ConfigWriteCap", ({
        dummy_field: bcs.bool()
    }));
}
export function ConfigKey() {
    return bcs.struct("ConfigKey", ({
        per_type_index: bcs.u64(),
        per_type_key: bcs.vector(bcs.u8())
    }));
}
export function AddressKey() {
    return bcs.struct("AddressKey", ({
        pos0: bcs.Address
    }));
}
export function GlobalPauseKey() {
    return bcs.struct("GlobalPauseKey", ({
        dummy_field: bcs.bool()
    }));
}
export function PerTypeConfigCreated() {
    return bcs.struct("PerTypeConfigCreated", ({
        key: ConfigKey(),
        config_id: bcs.Address
    }));
}
export function PerTypeList() {
    return bcs.struct("PerTypeList", ({
        id: object.UID(),
        denied_count: table.Table(),
        denied_addresses: table.Table()
    }));
}