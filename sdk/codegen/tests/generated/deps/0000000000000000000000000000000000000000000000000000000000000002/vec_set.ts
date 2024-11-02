import { bcs, type BcsType } from "@mysten/sui/bcs";
export function VecSet<T0 extends BcsType<any>>(...typeParameters: [
    T0
]) {
    return bcs.struct("VecSet", ({
        contents: bcs.vector(typeParameters[0])
    }));
}