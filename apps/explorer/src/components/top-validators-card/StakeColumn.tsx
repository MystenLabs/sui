// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SUI_TYPE_ARG } from '@mysten/sui.js';

import { useFormatCoin } from '~/hooks/useFormatCoin';
import { Text } from '~/ui/Text';

type StakeColumnProps = {
    stake: bigint;
}

export function StakeColumn({ stake }:  StakeColumnProps) {
    const [amount, symbol] = useFormatCoin(stake, SUI_TYPE_ARG);
    return (
        <div className="flex items-end gap-0.5">
            <Text variant="bodySmall/medium" color="steel-darker">
                {amount}
            </Text>
            <Text variant="captionSmall/medium" color="steel-dark">
                {symbol}
            </Text>
        </div>
    );
}