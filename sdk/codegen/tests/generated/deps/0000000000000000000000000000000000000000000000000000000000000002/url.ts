import { bcs } from "@mysten/sui/bcs";
export function Url() {
    return bcs.struct("Url", ({
        url: bcs.string()
    }));
}