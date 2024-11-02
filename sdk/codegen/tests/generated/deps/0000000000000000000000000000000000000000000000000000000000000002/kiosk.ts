import { bcs } from "@mysten/sui/bcs";
import * as object from "./object";
import * as balance from "./balance";
export function Kiosk() {
    return bcs.struct("Kiosk", ({
        id: object.UID(),
        profits: balance.Balance(),
        owner: bcs.Address,
        item_count: bcs.u32(),
        allow_extensions: bcs.bool()
    }));
}
export function KioskOwnerCap() {
    return bcs.struct("KioskOwnerCap", ({
        id: object.UID(),
        for: bcs.Address
    }));
}
export function PurchaseCap() {
    return bcs.struct("PurchaseCap", ({
        id: object.UID(),
        kiosk_id: bcs.Address,
        item_id: bcs.Address,
        min_price: bcs.u64()
    }));
}
export function Borrow() {
    return bcs.struct("Borrow", ({
        kiosk_id: bcs.Address,
        item_id: bcs.Address
    }));
}
export function Item() {
    return bcs.struct("Item", ({
        id: bcs.Address
    }));
}
export function Listing() {
    return bcs.struct("Listing", ({
        id: bcs.Address,
        is_exclusive: bcs.bool()
    }));
}
export function Lock() {
    return bcs.struct("Lock", ({
        id: bcs.Address
    }));
}
export function ItemListed() {
    return bcs.struct("ItemListed", ({
        kiosk: bcs.Address,
        id: bcs.Address,
        price: bcs.u64()
    }));
}
export function ItemPurchased() {
    return bcs.struct("ItemPurchased", ({
        kiosk: bcs.Address,
        id: bcs.Address,
        price: bcs.u64()
    }));
}
export function ItemDelisted() {
    return bcs.struct("ItemDelisted", ({
        kiosk: bcs.Address,
        id: bcs.Address
    }));
}