import { bcs, type BcsType } from "@mysten/sui/bcs";
export function VecMap<T0 extends BcsType<any>, T1 extends BcsType<any>>(...typeParameters: [
    T0,
    T1
]) {
    return bcs.struct("VecMap", ({
        contents: bcs.vector(Entry(typeParameters[0], typeParameters[1]))
    }));
}
export function Entry<T0 extends BcsType<any>, T1 extends BcsType<any>>(...typeParameters: [
    T0,
    T1
]) {
    return bcs.struct("Entry", ({
        key: typeParameters[0],
        value: typeParameters[1]
    }));
}