import { bcs } from "@mysten/sui/bcs";
import { type Transaction } from "@mysten/sui/transactions";
import { normalizeMoveArguments, type RawTransactionArgument } from "./utils/index.ts";
import * as object from "./deps/0000000000000000000000000000000000000000000000000000000000000002/object";
import * as table_vec from "./deps/0000000000000000000000000000000000000000000000000000000000000002/table_vec";
import * as balance from "./deps/0000000000000000000000000000000000000000000000000000000000000002/balance";
import * as policy from "./deps/0000000000000000000000000000000000000000000000000000000000000000/policy";
import * as coin from "./deps/0000000000000000000000000000000000000000000000000000000000000002/coin";
export function FeedAdminCap() {
    return bcs.struct("FeedAdminCap", ({
        id: object.UID(),
        feed: bcs.Address
    }));
}
export function PolicyCapKey() {
    return bcs.struct("PolicyCapKey", ({
        dummy_field: bcs.bool()
    }));
}
export function Feed() {
    return bcs.struct("Feed", ({
        id: object.UID(),
        version: bcs.u8(),
        publish_policy: bcs.Address,
        access_policy: bcs.Address,
        content: table_vec.TableVec(),
        price: bcs.u64(),
        balance: balance.Balance(),
        title: bcs.string(),
        description: bcs.string()
    }));
}
export function BlobId() {
    return bcs.struct("BlobId", ({
        pos0: bcs.u256()
    }));
}
export function FeedContent() {
    return bcs.struct("FeedContent", ({
        content: BlobId(),
        author: bcs.Address,
        sub_feed: bcs.option(bcs.Address)
    }));
}
export function FeedContentOption() {
    return bcs.enum("FeedContentOption", ({
        Some: FeedContent(),
        None: null
    }));
}
export function init(packageAddress: string) {
    function create_feed(options: {
        arguments: [
            RawTransactionArgument<string>,
            RawTransactionArgument<string>,
            RawTransactionArgument<string>,
            RawTransactionArgument<string>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::policy::Policy",
            "0000000000000000000000000000000000000000000000000000000000000000::policy::Policy",
            "0000000000000000000000000000000000000000000000000000000000000001::string::String",
            "0000000000000000000000000000000000000000000000000000000000000001::string::String"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "create_feed",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    function create_comment_feed(options: {
        arguments: [
            RawTransactionArgument<string>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::policy::Policy"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "create_comment_feed",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    function id(options: {
        arguments: [
            RawTransactionArgument<string>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::feed::Feed"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "id",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    function add_content(options: {
        arguments: [
            RawTransactionArgument<string>,
            RawTransactionArgument<string>,
            RawTransactionArgument<number | bigint>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::feed::Feed",
            "0000000000000000000000000000000000000000000000000000000000000000::policy::Policy",
            "u256"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "add_content",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    function add_content_with_subfeed(options: {
        arguments: [
            RawTransactionArgument<string>,
            RawTransactionArgument<string>,
            RawTransactionArgument<number | bigint>,
            RawTransactionArgument<string>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::feed::Feed",
            "0000000000000000000000000000000000000000000000000000000000000000::policy::Policy",
            "u256",
            "0000000000000000000000000000000000000000000000000000000000000000::feed::Feed"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "add_content_with_subfeed",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    function remove_content(options: {
        arguments: [
            RawTransactionArgument<string>,
            RawTransactionArgument<number | bigint>,
            RawTransactionArgument<string>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::feed::Feed",
            "u64",
            "0000000000000000000000000000000000000000000000000000000000000000::feed::FeedAdminCap"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "remove_content",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    function remove_own_content(options: {
        arguments: [
            RawTransactionArgument<string>,
            RawTransactionArgument<number | bigint>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::feed::Feed",
            "u64"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "remove_own_content",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    function set_price(options: {
        arguments: [
            RawTransactionArgument<string>,
            RawTransactionArgument<string>,
            RawTransactionArgument<string>,
            RawTransactionArgument<string>,
            RawTransactionArgument<number | bigint>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::feed::Feed",
            "0000000000000000000000000000000000000000000000000000000000000000::feed::FeedAdminCap",
            "0000000000000000000000000000000000000000000000000000000000000000::policy::Policy",
            "0000000000000000000000000000000000000000000000000000000000000000::policy::PolicyAdminCap",
            "u64"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "set_price",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    function purchase_access(options: {
        arguments: [
            RawTransactionArgument<string>,
            RawTransactionArgument<string>,
            RawTransactionArgument<string>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::feed::Feed",
            "0000000000000000000000000000000000000000000000000000000000000000::policy::Policy",
            "0000000000000000000000000000000000000000000000000000000000000002::coin::Coin<0000000000000000000000000000000000000000000000000000000000000002::sui::SUI>"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "purchase_access",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    function withdraw_balance(options: {
        arguments: [
            RawTransactionArgument<string>,
            RawTransactionArgument<string>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::feed::Feed",
            "0000000000000000000000000000000000000000000000000000000000000000::feed::FeedAdminCap"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "withdraw_balance",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    function share(options: {
        arguments: [
            RawTransactionArgument<string>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::feed::Feed"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "share",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    function validate_version(options: {
        arguments: [
            RawTransactionArgument<string>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::feed::Feed"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "validate_version",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    function validate_publish_policy(options: {
        arguments: [
            RawTransactionArgument<string>,
            RawTransactionArgument<string>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::feed::Feed",
            "0000000000000000000000000000000000000000000000000000000000000000::policy::Policy"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "validate_publish_policy",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    function validate_access_policy(options: {
        arguments: [
            RawTransactionArgument<string>,
            RawTransactionArgument<string>
        ];
    }) {
        const argumentsTypes = [
            "0000000000000000000000000000000000000000000000000000000000000000::feed::Feed",
            "0000000000000000000000000000000000000000000000000000000000000000::policy::Policy"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "validate_access_policy",
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
            "0000000000000000000000000000000000000000000000000000000000000000::feed::Feed",
            "0000000000000000000000000000000000000000000000000000000000000000::feed::FeedAdminCap"
        ];
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "validate_cap",
            arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
        });
    }
    return { create_feed, create_comment_feed, id, add_content, add_content_with_subfeed, remove_content, remove_own_content, set_price, purchase_access, withdraw_balance, share, validate_version, validate_publish_policy, validate_access_policy, validate_cap };
}