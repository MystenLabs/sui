// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormatCoin } from '@mysten/core';
import { type CoinStruct } from '@mysten/sui.js';

import { ObjectLink } from '~/ui/InternalLink';
import { Text } from '~/ui/Text';

type CoinItemProps = {
    coin: CoinStruct;
};

function CoinItem({ coin }: CoinItemProps): JSX.Element {
    const [formattedBalance, symbol] = useFormatCoin(
        coin.balance,
        coin.coinType
    );
    return (
        <div className="bg-grey-40 grid grid-flow-row auto-rows-fr grid-cols-4 items-center">
            <Text color="steel-darker" variant="bodySmall/medium">
                Object ID
            </Text>
            <div className="col-span-3">
                <ObjectLink objectId={coin.coinObjectId} noTruncate />
            </div>

            <Text color="steel-darker" variant="bodySmall/medium">
                Balance
            </Text>

            <div className="col-span-3 inline-flex items-end gap-1">
                <Text color="steel-darker" variant="bodySmall/medium">
                    {formattedBalance}
                </Text>
                <Text color="steel" variant="subtitleSmallExtra/normal">
                    {symbol}
                </Text>
            </div>
        </div>
    );
}

export default CoinItem;
