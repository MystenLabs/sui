import { bcs, type BcsType } from "@mysten/sui/bcs";
import { type Transaction } from "@mysten/sui/transactions";
import * as object from "./deps/0000000000000000000000000000000000000000000000000000000000000002/object";
export function Box<T0 extends BcsType<any>>(...typeParameters: [
    T0
]) {
    return bcs.struct("Box", ({
        id: object.UID(),
        value: typeParameters[0]
    }));
}
export function init(packageAddress: string) {
    function create_box(options: {
        arguments: [
            T0
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "other",
            function: "create_box",
            arguments: options.arguments,
        });
    }
    function box_id(options: {
        arguments: [
            ReturnType<typeof Box<T0>>["$inferType"]
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "other",
            function: "box_id",
            arguments: options.arguments,
        });
    }
    return { create_box, box_id };
}