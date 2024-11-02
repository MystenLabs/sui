import { bcs } from "@mysten/sui/bcs";
import * as object from "./object";
export function AuthenticatorState() {
    return bcs.struct("AuthenticatorState", ({
        id: object.UID(),
        version: bcs.u64()
    }));
}
export function AuthenticatorStateInner() {
    return bcs.struct("AuthenticatorStateInner", ({
        version: bcs.u64(),
        active_jwks: bcs.vector(ActiveJwk())
    }));
}
export function JWK() {
    return bcs.struct("JWK", ({
        kty: bcs.string(),
        e: bcs.string(),
        n: bcs.string(),
        alg: bcs.string()
    }));
}
export function JwkId() {
    return bcs.struct("JwkId", ({
        iss: bcs.string(),
        kid: bcs.string()
    }));
}
export function ActiveJwk() {
    return bcs.struct("ActiveJwk", ({
        jwk_id: JwkId(),
        jwk: JWK(),
        epoch: bcs.u64()
    }));
}