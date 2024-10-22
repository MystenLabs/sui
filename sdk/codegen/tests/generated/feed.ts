import { bcs } from "@mysten/sui/bcs";
import { type Transaction } from "@mysten/sui/transactions";
import * as object from "./deps/0000000000000000000000000000000000000000000000000000000000000002/object";
import * as table_vec from "./deps/0000000000000000000000000000000000000000000000000000000000000002/table_vec";
import * as balance from "./deps/0000000000000000000000000000000000000000000000000000000000000002/balance";
import * as sui from "./deps/0000000000000000000000000000000000000000000000000000000000000002/sui";
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
        content: table_vec.TableVec(FeedContentOption()),
        price: bcs.u64(),
        balance: balance.Balance(sui.SUI()),
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
            ReturnType<typeof policy.Policy>["$inferType"],
            ReturnType<typeof policy.Policy>["$inferType"],
            string,
            string
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "create_feed",
            arguments: options.arguments,
        });
    }
    function create_comment_feed(options: {
        arguments: [
            ReturnType<typeof policy.Policy>["$inferType"]
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "create_comment_feed",
            arguments: options.arguments,
        });
    }
    function id(options: {
        arguments: [
            ReturnType<typeof Feed>["$inferType"]
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "id",
            arguments: options.arguments,
        });
    }
    function add_content(options: {
        arguments: [
            ReturnType<typeof Feed>["$inferType"],
            ReturnType<typeof policy.Policy>["$inferType"],
            number
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "add_content",
            arguments: options.arguments,
        });
    }
    function add_content_with_subfeed(options: {
        arguments: [
            ReturnType<typeof Feed>["$inferType"],
            ReturnType<typeof policy.Policy>["$inferType"],
            number,
            ReturnType<typeof Feed>["$inferType"]
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "add_content_with_subfeed",
            arguments: options.arguments,
        });
    }
    function remove_content(options: {
        arguments: [
            ReturnType<typeof Feed>["$inferType"],
            number,
            ReturnType<typeof FeedAdminCap>["$inferType"]
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "remove_content",
            arguments: options.arguments,
        });
    }
    function remove_own_content(options: {
        arguments: [
            ReturnType<typeof Feed>["$inferType"],
            number
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "remove_own_content",
            arguments: options.arguments,
        });
    }
    function set_price(options: {
        arguments: [
            ReturnType<typeof Feed>["$inferType"],
            ReturnType<typeof FeedAdminCap>["$inferType"],
            ReturnType<typeof policy.Policy>["$inferType"],
            ReturnType<typeof policy.PolicyAdminCap>["$inferType"],
            number
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "set_price",
            arguments: options.arguments,
        });
    }
    function purchase_access(options: {
        arguments: [
            ReturnType<typeof Feed>["$inferType"],
            ReturnType<typeof policy.Policy>["$inferType"],
            ReturnType<typeof coin.Coin<ReturnType<typeof sui.SUI>["$inferType"]>>["$inferType"]
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "purchase_access",
            arguments: options.arguments,
        });
    }
    function withdraw_balance(options: {
        arguments: [
            ReturnType<typeof Feed>["$inferType"],
            ReturnType<typeof FeedAdminCap>["$inferType"]
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "withdraw_balance",
            arguments: options.arguments,
        });
    }
    function share(options: {
        arguments: [
            ReturnType<typeof Feed>["$inferType"]
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "share",
            arguments: options.arguments,
        });
    }
    function validate_version(options: {
        arguments: [
            ReturnType<typeof Feed>["$inferType"]
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "validate_version",
            arguments: options.arguments,
        });
    }
    function validate_publish_policy(options: {
        arguments: [
            ReturnType<typeof Feed>["$inferType"],
            ReturnType<typeof policy.Policy>["$inferType"]
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "validate_publish_policy",
            arguments: options.arguments,
        });
    }
    function validate_access_policy(options: {
        arguments: [
            ReturnType<typeof Feed>["$inferType"],
            ReturnType<typeof policy.Policy>["$inferType"]
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "validate_access_policy",
            arguments: options.arguments,
        });
    }
    function validate_cap(options: {
        arguments: [
            ReturnType<typeof Feed>["$inferType"],
            ReturnType<typeof FeedAdminCap>["$inferType"]
        ];
    }) {
        return (tx: Transaction) => tx.moveCall({
            package: packageAddress,
            module: "feed",
            function: "validate_cap",
            arguments: options.arguments,
        });
    }
    return { create_feed, create_comment_feed, id, add_content, add_content_with_subfeed, remove_content, remove_own_content, set_price, purchase_access, withdraw_balance, share, validate_version, validate_publish_policy, validate_access_policy, validate_cap };
}