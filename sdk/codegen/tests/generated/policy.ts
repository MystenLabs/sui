import { bcs } from "@mysten/sui/bcs";
import { type Transaction } from "@mysten/sui/transactions";
import * as object from "./deps/0000000000000000000000000000000000000000000000000000000000000002/object";
import * as table from "./deps/0000000000000000000000000000000000000000000000000000000000000002/table";
export function PolicyAdminCap() {
    return bcs.struct("PolicyAdminCap", ({
        id: object.UID(),
        policy: bcs.Address
    }));
}
export function Policy() {
    return bcs.struct("Policy", ({
        id: object.UID(),
        public_key: bcs.vector(bcs.u8()),
        private_keys: table.Table(bcs.vector(bcs.u8()), bcs.vector(bcs.u8())),
        version: bcs.u8(),
        authorized: table.Table(bcs.Address, bcs.bool()),
        signature: bcs.option(bcs.vector(bcs.u8()))
    }));
}
export function init(packageAddress: string) {
    function new_policy(options: {
        arguments: [
            number[]
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "policy",
            function: "new_policy",
            arguments: options.arguments,
        });
    }
    function share(options: {
        arguments: [
            ReturnType<typeof Policy>["$inferType"]
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "policy",
            function: "share",
            arguments: options.arguments,
        });
    }
    function policy_id(options: {
        arguments: [
            ReturnType<typeof PolicyAdminCap>["$inferType"]
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "policy",
            function: "policy_id",
            arguments: options.arguments,
        });
    }
    function add_private_key(options: {
        arguments: [
            ReturnType<typeof Policy>["$inferType"],
            ReturnType<typeof PolicyAdminCap>["$inferType"],
            number[],
            number[]
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "policy",
            function: "add_private_key",
            arguments: options.arguments,
        });
    }
    function add_signature(options: {
        arguments: [
            ReturnType<typeof Policy>["$inferType"],
            ReturnType<typeof PolicyAdminCap>["$inferType"],
            number[]
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "policy",
            function: "add_signature",
            arguments: options.arguments,
        });
    }
    function id(options: {
        arguments: [
            ReturnType<typeof Policy>["$inferType"]
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "policy",
            function: "id",
            arguments: options.arguments,
        });
    }
    function create_cap(options: {
        arguments: [
            ReturnType<typeof Policy>["$inferType"],
            ReturnType<typeof PolicyAdminCap>["$inferType"]
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "policy",
            function: "create_cap",
            arguments: options.arguments,
        });
    }
    function destroy_cap(options: {
        arguments: [
            ReturnType<typeof PolicyAdminCap>["$inferType"]
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "policy",
            function: "destroy_cap",
            arguments: options.arguments,
        });
    }
    function is_authorized(options: {
        arguments: [
            ReturnType<typeof Policy>["$inferType"],
            string
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "policy",
            function: "is_authorized",
            arguments: options.arguments,
        });
    }
    function authorize(options: {
        arguments: [
            ReturnType<typeof Policy>["$inferType"],
            ReturnType<typeof PolicyAdminCap>["$inferType"],
            string
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "policy",
            function: "authorize",
            arguments: options.arguments,
        });
    }
    function deauthorize(options: {
        arguments: [
            ReturnType<typeof Policy>["$inferType"],
            ReturnType<typeof PolicyAdminCap>["$inferType"],
            string
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "policy",
            function: "deauthorize",
            arguments: options.arguments,
        });
    }
    function validate_version(options: {
        arguments: [
            ReturnType<typeof Policy>["$inferType"]
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "policy",
            function: "validate_version",
            arguments: options.arguments,
        });
    }
    function validate_cap(options: {
        arguments: [
            ReturnType<typeof Policy>["$inferType"],
            ReturnType<typeof PolicyAdminCap>["$inferType"]
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "policy",
            function: "validate_cap",
            arguments: options.arguments,
        });
    }
    return { new_policy, share, policy_id, add_private_key, add_signature, id, create_cap, destroy_cap, is_authorized, authorize, deauthorize, validate_version, validate_cap };
}