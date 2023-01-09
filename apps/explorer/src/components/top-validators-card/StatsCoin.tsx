// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SUI_TYPE_ARG } from '@mysten/sui.js';

import { useFormatCoin } from '~/hooks/useFormatCoin';
import { Heading } from '~/ui/Heading';

export function StatsCoin({ amount }: { amount: bigint }) {
    const [formattedAmount] = useFormatCoin(amount, SUI_TYPE_ARG);
    return (
        <Heading as="h3" variant="heading2/semibold" color="steel-darker">
            {formattedAmount}
        </Heading>
    );
}
