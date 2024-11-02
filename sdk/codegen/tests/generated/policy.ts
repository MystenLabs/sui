import { bcs } from "@mysten/sui/bcs";
import { type Transaction } from "@mysten/sui/transactions";
import { normalizeMoveArguments, type RawTransactionArgument } from "./utils/index.ts";
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
        private_keys: table.Table(),
        version: bcs.u8(),
        authorized: table.Table(),
        signature: bcs.option(bcs.vector(bcs.u8()))
    }));
}
export function init(packageAddress: string) {
    function new_policy(options: {
        arguments: [
            RawTransactionArgument<number[]>
        ];
    }) {
        const argumentsTypes = [
            "vector<u8>"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "policy",
            function: "new_policy",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    function share(options: {
        arguments: [
            RawTransactionArgument<string>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::policy::Policy"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "policy",
            function: "share",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    function policy_id(options: {
        arguments: [
            RawTransactionArgument<string>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::policy::PolicyAdminCap"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "policy",
            function: "policy_id",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    function add_private_key(options: {
        arguments: [
            RawTransactionArgument<string>,
            RawTransactionArgument<string>,
            RawTransactionArgument<number[]>,
            RawTransactionArgument<number[]>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::policy::Policy",
            "0000000000000000000000000000000000000000000000000000000000000000::policy::PolicyAdminCap",
            "vector<u8>",
            "vector<u8>"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "policy",
            function: "add_private_key",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    function add_signature(options: {
        arguments: [
            RawTransactionArgument<string>,
            RawTransactionArgument<string>,
            RawTransactionArgument<number[]>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::policy::Policy",
            "0000000000000000000000000000000000000000000000000000000000000000::policy::PolicyAdminCap",
            "vector<u8>"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "policy",
            function: "add_signature",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    function id(options: {
        arguments: [
            RawTransactionArgument<string>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::policy::Policy"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "policy",
            function: "id",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    function create_cap(options: {
        arguments: [
            RawTransactionArgument<string>,
            RawTransactionArgument<string>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::policy::Policy",
            "0000000000000000000000000000000000000000000000000000000000000000::policy::PolicyAdminCap"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "policy",
            function: "create_cap",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    function destroy_cap(options: {
        arguments: [
            RawTransactionArgument<string>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::policy::PolicyAdminCap"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "policy",
            function: "destroy_cap",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    function is_authorized(options: {
        arguments: [
            RawTransactionArgument<string>,
            RawTransactionArgument<string>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::policy::Policy",
            "address"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "policy",
            function: "is_authorized",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
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
            "0000000000000000000000000000000000000000000000000000000000000000::policy::Policy",
            "0000000000000000000000000000000000000000000000000000000000000000::policy::PolicyAdminCap",
            "address"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "policy",
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
            "0000000000000000000000000000000000000000000000000000000000000000::policy::Policy",
            "0000000000000000000000000000000000000000000000000000000000000000::policy::PolicyAdminCap",
            "address"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "policy",
            function: "deauthorize",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    function validate_version(options: {
        arguments: [
            RawTransactionArgument<string>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::policy::Policy"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "policy",
            function: "validate_version",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    function validate_cap(options: {
        arguments: [
            RawTransactionArgument<string>,
            RawTransactionArgument<string>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::policy::Policy",
            "0000000000000000000000000000000000000000000000000000000000000000::policy::PolicyAdminCap"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "policy",
            function: "validate_cap",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    return { new_policy, share, policy_id, add_private_key, add_signature, id, create_cap, destroy_cap, is_authorized, authorize, deauthorize, validate_version, validate_cap };
}