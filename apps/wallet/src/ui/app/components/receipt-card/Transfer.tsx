// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Heading } from '_app/shared/heading';
import { Text } from '_app/shared/text';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import { useMiddleEllipsis, useFormatCoin } from '_hooks';

const TRUNCATE_MAX_LENGTH = 8;
const TRUNCATE_PREFIX_LENGTH = 4;

type TransferWithProps = {
    address: string;
    amount: number | bigint | null;
    // TODO add more labels
    isSender: boolean;
    coinType: string | null;
    accountAddress: string;
};

export function Transfer({
    address,
    amount,
    isSender,
    coinType,
    accountAddress,
}: TransferWithProps) {
    const receiverAddress = useMiddleEllipsis(
        address,
        TRUNCATE_MAX_LENGTH,
        TRUNCATE_PREFIX_LENGTH
    );

    const [formattedAmount, symbol] = useFormatCoin(amount || 0, coinType);

    return (
        <div className="flex flex-col justify-between items-center pt-3.5 gap-3.5">
            {coinType && (
                <div className="flex items-center w-full justify-between">
                    <div className="flex gap-0.5 items-center leading-none">
                        <Text variant="body" weight="medium" color="steel-dark">
                            {isSender ? 'Sent' : 'Received'}
                        </Text>
                    </div>
                    <div className="flex gap-0.5">
                        <Heading variant="heading2" as="div">
                            {formattedAmount}
                        </Heading>
                        <Text
                            variant="body"
                            weight="medium"
                            color="steel-darker"
                        >
                            {symbol}
                        </Text>
                    </div>
                </div>
            )}

            {accountAddress !== address && (
                <div className="flex items-center w-full justify-between">
                    <Text variant="body" weight="medium" color="steel-darker">
                        {isSender ? 'To' : 'From'}
                    </Text>

                    <ExplorerLink
                        type={ExplorerLinkType.address}
                        address={address}
                        className="text-sui-dark no-underline font-semibold uppercase text-caption "
                        showIcon={false}
                    >
                        {receiverAddress}
                    </ExplorerLink>
                </div>
            )}
        </div>
    );
}
