import { bcs, type BcsType } from "@mysten/sui/bcs";
export function Referent<T0 extends BcsType<any>>(...typeParameters: [
    T0
]) {
    return bcs.struct("Referent", ({
        id: bcs.Address,
        value: bcs.option(typeParameters[0])
    }));
}
export function Borrow() {
    return bcs.struct("Borrow", ({
        ref: bcs.Address,
        obj: bcs.Address
    }));
}