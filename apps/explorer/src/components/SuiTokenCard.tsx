// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Sui } from '@mysten/icons';

import { Card } from '~/ui/Card';
import { Heading } from '~/ui/Heading';
import { Text } from '~/ui/Text';

export function SuiTokenCard() {
    return (
        <Card bg="lightBlue" spacing="lg">
            <div className="flex items-center gap-2">
                <div className="h-4.5 w-4.5 items-center justify-center rounded-full bg-sui p-1">
                    <Sui className="h-full w-full text-white" />
                </div>
                <Heading
                    as="div"
                    variant="heading4/semibold"
                    color="steel-darker"
                >
                    1 SUI = $0.33
                </Heading>
            </div>
            <div className="flex gap-8 mt-8">
                <MarketData
                    title="Market Cap"
                    amount="4.69 B"
                    amountSymbol="USD"
                />
                <MarketData
                    title="Total Supply"
                    amount="10 B"
                    amountSymbol="SUI"
                />
            </div>
        </Card>
    );
}

type MarketDataProps = {
    title: string;
    amount: string;
    amountSymbol: string;
};

function MarketData({ title, amount, amountSymbol }: MarketDataProps) {
    return (
        <div>
            <Text variant="caption/semibold" color="steel-dark">
                {title}
            </Text>
            <div className="mt-1.5 flex items-center gap-0.5">
                <Heading
                    as="div"
                    variant="heading3/semibold"
                    color="steel-darker"
                >
                    {amount}
                </Heading>
                <Heading
                    as="div"
                    variant="heading4/semibold"
                    color="steel-darker"
                >
                    {amountSymbol}
                </Heading>
            </div>
        </div>
    );
}
