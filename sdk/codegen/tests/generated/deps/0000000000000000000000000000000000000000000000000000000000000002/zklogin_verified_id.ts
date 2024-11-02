import { bcs } from "@mysten/sui/bcs";
import * as object from "./object";
export function VerifiedID() {
    return bcs.struct("VerifiedID", ({
        id: object.UID(),
        owner: bcs.Address,
        key_claim_name: bcs.string(),
        key_claim_value: bcs.string(),
        issuer: bcs.string(),
        audience: bcs.string()
    }));
}