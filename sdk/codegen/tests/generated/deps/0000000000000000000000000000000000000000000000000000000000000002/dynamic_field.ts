import { bcs, type BcsType } from "@mysten/sui/bcs";
import * as object from "./object";
export function Field<T0 extends BcsType<any>, T1 extends BcsType<any>>(...typeParameters: [
    T0,
    T1
]) {
    return bcs.struct("Field", ({
        id: object.UID(),
        name: typeParameters[0],
        value: typeParameters[1]
    }));
}