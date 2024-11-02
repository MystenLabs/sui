import { bcs } from "@mysten/sui/bcs";
import { type Transaction } from "@mysten/sui/transactions";
import { normalizeMoveArguments, type RawTransactionArgument } from "./utils/index.ts";
import * as object from "./deps/0000000000000000000000000000000000000000000000000000000000000002/object";
import * as bag from "./deps/0000000000000000000000000000000000000000000000000000000000000002/bag";
import * as vec_set from "./deps/0000000000000000000000000000000000000000000000000000000000000002/vec_set";
export function EnokiObjects() {
    return bcs.struct("EnokiObjects", ({
        id: object.UID(),
        version: bcs.u8(),
        managed_objects: bag.Bag(),
        authorized: vec_set.VecSet(bcs.Address)
    }));
}
export function EnokiObjectsCap() {
    return bcs.struct("EnokiObjectsCap", ({
        id: object.UID()
    }));
}
export function EnokiManagedKey() {
    return bcs.struct("EnokiManagedKey", ({
        id: bcs.Address
    }));
}
export function EnokiManagedValue() {
    return bcs.struct("EnokiManagedValue", ({
        custom_id: bcs.string(),
        owner: bcs.Address,
        storage: object.UID()
    }));
}
export function ObjectKey() {
    return bcs.struct("ObjectKey", ({
        dummy_field: bcs.bool()
    }));
}
export function ReturnPromise() {
    return bcs.struct("ReturnPromise", ({
        id: bcs.Address
    }));
}
export function init(packageAddress: string) {
    function init(options: {
        arguments: [
        ];
    }) {
        const argumentsTypes = [];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "managed",
            function: "init",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    function attach_object<T0 extends BcsType<any>>(options: {
        arguments: [
            RawTransactionArgument<string>,
            RawTransactionArgument<T0>,
            RawTransactionArgument<string>
        ];
        typeArguments: [
            string
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::managed::EnokiObjects",
            `${options.typeArguments[0]}`,
            "0000000000000000000000000000000000000000000000000000000000000001::string::String"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "managed",
            function: "attach_object",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
            typeArguments: options.typeArguments
        });
    }
    function reclaim_object<T0 extends BcsType<any>>(options: {
        arguments: [
            RawTransactionArgument<string>,
            RawTransactionArgument<string>
        ];
        typeArguments: [
            string
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::managed::EnokiObjects",
            "0000000000000000000000000000000000000000000000000000000000000002::object::ID"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "managed",
            function: "reclaim_object",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
            typeArguments: options.typeArguments
        });
    }
    function borrow<T0 extends BcsType<any>>(options: {
        arguments: [
            RawTransactionArgument<string>,
            RawTransactionArgument<string>
        ];
        typeArguments: [
            string
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::managed::EnokiObjects",
            "0000000000000000000000000000000000000000000000000000000000000002::object::ID"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "managed",
            function: "borrow",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
            typeArguments: options.typeArguments
        });
    }
    function borrow_with_custom_id<T0 extends BcsType<any>>(options: {
        arguments: [
            RawTransactionArgument<string>,
            RawTransactionArgument<string>,
            RawTransactionArgument<string>
        ];
        typeArguments: [
            string
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::managed::EnokiObjects",
            "0000000000000000000000000000000000000000000000000000000000000002::object::ID",
            "0000000000000000000000000000000000000000000000000000000000000001::string::String"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "managed",
            function: "borrow_with_custom_id",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
            typeArguments: options.typeArguments
        });
    }
    function put_back<T0 extends BcsType<any>>(options: {
        arguments: [
            RawTransactionArgument<string>,
            RawTransactionArgument<T0>,
            RawTransactionArgument<string>
        ];
        typeArguments: [
            string
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::managed::EnokiObjects",
            `${options.typeArguments[0]}`,
            "0000000000000000000000000000000000000000000000000000000000000000::managed::ReturnPromise"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "managed",
            function: "put_back",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
            typeArguments: options.typeArguments
        });
    }
    function authorize(options: {
        arguments: [
            RawTransactionArgument<string>,
            RawTransactionArgument<string>,
            RawTransactionArgument<string>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::managed::EnokiObjects",
            "0000000000000000000000000000000000000000000000000000000000000000::managed::EnokiObjectsCap",
            "address"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "managed",
            function: "authorize",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    function deauthorize(options: {
        arguments: [
            RawTransactionArgument<string>,
            RawTransactionArgument<string>,
            RawTransactionArgument<string>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::managed::EnokiObjects",
            "0000000000000000000000000000000000000000000000000000000000000000::managed::EnokiObjectsCap",
            "address"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "managed",
            function: "deauthorize",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    function deauthorize_self(options: {
        arguments: [
            RawTransactionArgument<string>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::managed::EnokiObjects"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "managed",
            function: "deauthorize_self",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    function update_version(options: {
        arguments: [
            RawTransactionArgument<string>,
            RawTransactionArgument<string>,
            RawTransactionArgument<number>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::managed::EnokiObjects",
            "0000000000000000000000000000000000000000000000000000000000000000::managed::EnokiObjectsCap",
            "u8"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "managed",
            function: "update_version",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    function internal_get_value(options: {
        arguments: [
            RawTransactionArgument<string>,
            RawTransactionArgument<string>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::managed::EnokiObjects",
            "0000000000000000000000000000000000000000000000000000000000000002::object::ID"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "managed",
            function: "internal_get_value",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    function validate_version(options: {
        arguments: [
            RawTransactionArgument<string>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::managed::EnokiObjects"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "managed",
            function: "validate_version",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    return { init, attach_object, reclaim_object, borrow, borrow_with_custom_id, put_back, authorize, deauthorize, deauthorize_self, update_version, internal_get_value, validate_version };
}