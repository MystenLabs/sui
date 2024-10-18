import { bcs, type BcsType } from "@mysten/sui/bcs";
export function mod() {
    function Box<T0 extends BcsType<any>>(...typeParameters: [
        T0
    ]) {
        return bcs.struct("Box", ({
            id: object.UID(),
            value: typeParameters[0]
        }));
    }
    return ({
        Box
    });
}