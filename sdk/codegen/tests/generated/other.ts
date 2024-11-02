import { bcs, type BcsType } from "@mysten/sui/bcs";
import { type Transaction } from "@mysten/sui/transactions";
import { normalizeMoveArguments, type RawTransactionArgument } from "./utils/index.ts";
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
    function create_box<T0 extends BcsType<any>>(options: {
        arguments: [
            RawTransactionArgument<T0>
        ];
        typeArguments: [
            string
        ];
    }) {
        const argumentsTypes = [
            `${options.typeArguments[0]}`
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "other",
            function: "create_box",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
            typeArguments: options.typeArguments
        });
    }
    function box_id<T0 extends BcsType<any>>(options: {
        arguments: [
            RawTransactionArgument<string>
        ];
        typeArguments: [
            string
        ];
    }) {
        const argumentsTypes = [
            `0000000000000000000000000000000000000000000000000000000000000000::other::Box<${options.typeArguments[0]}>`
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "other",
            function: "box_id",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
            typeArguments: options.typeArguments
        });
    }
    function bitwise_not(options: {
        arguments: [
            RawTransactionArgument<number | bigint>
        ];
    }) {
        const argumentsTypes = [
            "u64"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "other",
            function: "bitwise_not",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    return { create_box, box_id, bitwise_not };
}