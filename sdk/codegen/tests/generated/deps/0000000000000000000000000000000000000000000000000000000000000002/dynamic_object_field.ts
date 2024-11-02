import { bcs, type BcsType } from "@mysten/sui/bcs";
export function Wrapper<T0 extends BcsType<any>>(...typeParameters: [
    T0
]) {
    return bcs.struct("Wrapper", ({
        name: typeParameters[0]
    }));
}