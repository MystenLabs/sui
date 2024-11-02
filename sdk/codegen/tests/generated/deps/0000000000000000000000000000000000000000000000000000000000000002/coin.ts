import { bcs } from "@mysten/sui/bcs";
import * as object from "./object";
import * as balance from "./balance";
import * as url from "./url";
export function Coin() {
    return bcs.struct("Coin", ({
        id: object.UID(),
        balance: balance.Balance()
    }));
}
export function CoinMetadata() {
    return bcs.struct("CoinMetadata", ({
        id: object.UID(),
        decimals: bcs.u8(),
        name: bcs.string(),
        symbol: bcs.string(),
        description: bcs.string(),
        icon_url: bcs.option(url.Url())
    }));
}
export function RegulatedCoinMetadata() {
    return bcs.struct("RegulatedCoinMetadata", ({
        id: object.UID(),
        coin_metadata_object: bcs.Address,
        deny_cap_object: bcs.Address
    }));
}
export function TreasuryCap() {
    return bcs.struct("TreasuryCap", ({
        id: object.UID(),
        total_supply: balance.Supply()
    }));
}
export function DenyCapV2() {
    return bcs.struct("DenyCapV2", ({
        id: object.UID(),
        allow_global_pause: bcs.bool()
    }));
}
export function CurrencyCreated() {
    return bcs.struct("CurrencyCreated", ({
        decimals: bcs.u8()
    }));
}
export function DenyCap() {
    return bcs.struct("DenyCap", ({
        id: object.UID()
    }));
}