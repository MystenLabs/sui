import { bcs, type BcsType } from "@mysten/sui/bcs";
import * as object from "./object";
export function LinkedTable<T0 extends BcsType<any>>(...typeParameters: [
    T0
]) {
    return bcs.struct("LinkedTable", ({
        id: object.UID(),
        size: bcs.u64(),
        head: bcs.option(typeParameters[0]),
        tail: bcs.option(typeParameters[0])
    }));
}
export function Node<T0 extends BcsType<any>, T1 extends BcsType<any>>(...typeParameters: [
    T0,
    T1
]) {
    return bcs.struct("Node", ({
        prev: bcs.option(typeParameters[0]),
        next: bcs.option(typeParameters[0]),
        value: typeParameters[1]
    }));
}