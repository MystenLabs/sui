import { bcs } from "@mysten/sui/bcs";
export function mod() {
    function FeedAdminCap() {
        return bcs.struct("FeedAdminCap", ({
            id: object.UID(),
            feed: bcs.Address
        }));
    }
    function PolicyCapKey() {
        return bcs.struct("PolicyCapKey", ({
            dummy_field: bcs.bool()
        }));
    }
    function Feed() {
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
    function BlobId() {
        return bcs.struct("BlobId", ({
            pos0: bcs.u256()
        }));
    }
    function FeedContent() {
        return bcs.struct("FeedContent", ({
            content: BlobId(),
            author: bcs.Address,
            sub_feed: bcs.option(bcs.Address)
        }));
    }
    function FeedContentOption() {
        return bcs.enum("FeedContentOption", ({
            Some: FeedContent(),
            None: null
        }));
    }
    return ({
        FeedAdminCap,
        PolicyCapKey,
        Feed,
        BlobId,
        FeedContent,
        FeedContentOption
    });
}