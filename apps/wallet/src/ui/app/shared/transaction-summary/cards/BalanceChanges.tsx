// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import {
    type BalanceChangeSummary,
    CoinFormat,
    useFormatCoin,
} from '@mysten/core';
import { formatAddress } from '@mysten/sui.js';
import { useState } from 'react';

import { Card } from '../Card';
import ExplorerLink from '_src/ui/app/components/explorer-link';
import { ExplorerLinkType } from '_src/ui/app/components/explorer-link/ExplorerLinkType';
import { useActiveAddress } from '_src/ui/app/hooks';
import { Text } from '_src/ui/app/shared/text';

interface BalanceChangesProps {
    changes?: BalanceChangeSummary[] | null;
}

function BalanceChangeEntry({ change }: { change: BalanceChangeSummary }) {
    const address = useActiveAddress();
    const [showAddress, setShowAddress] = useState(false);

    const { amount, coinType, owner } = change;
    const isPositive = BigInt(amount) > 0n;

    const [formatted, symbol] = useFormatCoin(
        amount,
        coinType,
        CoinFormat.FULL
    );

    return (
        <div className="flex flex-col gap-2 only:pt-0 last:pb-0 only:pb-0 first:pt-0 py-2">
            <div className="flex flex-col gap-2">
                <div className="flex justify-between">
                    <Text variant="pBodySmall" color="steel-dark">
                        Amount
                    </Text>
                    <div className="flex">
                        <Text
                            variant="pBodySmall"
                            color={isPositive ? 'success-dark' : 'issue-dark'}
                        >
                            {isPositive ? '+' : ''}
                            {formatted} {symbol}
                        </Text>
                    </div>
                </div>
                {owner && (
                    <div className="flex justify-between">
                        <Text variant="pBodySmall" color="steel-dark">
                            Owner
                        </Text>
                        <div
                            className="cursor-pointer text-hero-dark"
                            onClick={() => setShowAddress((prev) => !prev)}
                        >
                            {owner === address ? (
                                !showAddress ? (
                                    'You'
                                ) : (
                                    formatAddress(owner)
                                )
                            ) : (
                                <ExplorerLink
                                    type={ExplorerLinkType.address}
                                    address={owner}
                                    className="text-hero-dark no-underline"
                                >
                                    <Text variant="pBodySmall" truncate mono>
                                        {formatAddress(owner)}
                                    </Text>
                                </ExplorerLink>
                            )}
                        </div>
                    </div>
                )}
            </div>
        </div>
    );
}

export function BalanceChanges({ changes }: BalanceChangesProps) {
    if (!changes) return null;
    return (
        <Card heading="Balance Changes">
            <div className="divide-solid divide-x-0 divide-y divide-gray-40">
                {changes.map((change) => {
                    return <BalanceChangeEntry change={change} />;
                })}
            </div>
        </Card>
    );
}
