// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormatCoin } from '_hooks';
import { Heading } from '_src/ui/app/shared/heading';
import { Text } from '_src/ui/app/shared/text';

type TxnAmountProps = {
    amount: string | number;
    coinType: string;
    label: string;
};

export function TxnAmount({ amount, coinType, label }: TxnAmountProps) {
    const [formatAmount, symbol] = useFormatCoin(Math.abs(+amount), coinType);
    return (
        <div className="flex justify-between w-full items-center pt-3.5">
            <Text variant="body" weight="medium" color="steel-darker">
                {label}
            </Text>
            <div className="flex gap-1 items-center">
                <Heading variant="heading2" weight="semibold" color="gray-90">
                    {formatAmount}
                </Heading>
                <Text variant="body" weight="medium" color="steel-darker">
                    {symbol}
                </Text>
            </div>
        </div>
    );
}
