import { bcs } from "@mysten/sui/bcs";
export function ID() {
    return bcs.struct("ID", ({
        bytes: bcs.Address
    }));
}
export function UID() {
    return bcs.struct("UID", ({
        id: bcs.Address
    }));
}