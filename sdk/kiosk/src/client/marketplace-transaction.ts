// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ItemId } from "../types";
import { KioskTransaction } from "./kiosk-transaction";
import type { KioskTransactionParams } from "./kiosk-transaction";

export class MarketplaceTransaction extends KioskTransaction {
    protected marketType: string;

    constructor({ transactionBlock, kioskClient, cap, marketType }: KioskTransactionParams & { marketType: string }) {
        super({ transactionBlock, kioskClient, cap });
        this.marketType = marketType;
    }

    /**
     * Fixed Price Module
     * Calls: market_adapter::fixed_price::list<T, Market>()
     */
    list({ itemType, item, price }: ItemId & { price: string | bigint }) {
        this.transactionBlock.moveCall({
            target: '0x2::fixed_price::list',
            arguments: [
                this.kiosk!,
                this.kioskCap!,


            ],
            typeArguments: [ itemType, this.marketType ]
        })
    }
}
