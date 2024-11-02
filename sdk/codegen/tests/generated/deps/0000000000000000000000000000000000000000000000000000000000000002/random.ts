import { bcs } from "@mysten/sui/bcs";
import * as object from "./object";
import * as versioned from "./versioned";
export function Random() {
    return bcs.struct("Random", ({
        id: object.UID(),
        inner: versioned.Versioned()
    }));
}
export function RandomInner() {
    return bcs.struct("RandomInner", ({
        version: bcs.u64(),
        epoch: bcs.u64(),
        randomness_round: bcs.u64(),
        random_bytes: bcs.vector(bcs.u8())
    }));
}
export function RandomGenerator() {
    return bcs.struct("RandomGenerator", ({
        seed: bcs.vector(bcs.u8()),
        counter: bcs.u16(),
        buffer: bcs.vector(bcs.u8())
    }));
}