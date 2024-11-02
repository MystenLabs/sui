import { bcs } from "@mysten/sui/bcs";
import * as object from "./object";
export function VerifiedIssuer() {
    return bcs.struct("VerifiedIssuer", ({
        id: object.UID(),
        owner: bcs.Address,
        issuer: bcs.string()
    }));
}