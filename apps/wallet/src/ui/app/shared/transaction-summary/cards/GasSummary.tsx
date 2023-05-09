// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { type GasSummaryType, useFormatCoin } from '@mysten/core';
import { formatAddress } from '@mysten/sui.js';

import { Text } from '../../text';
import { Card } from '../Card';
import ExplorerLink from '_src/ui/app/components/explorer-link';
import { ExplorerLinkType } from '_src/ui/app/components/explorer-link/ExplorerLinkType';
import { GAS_TYPE_ARG } from '_src/ui/app/redux/slices/sui-objects/Coin';

export function GasSummary({ gasSummary }: { gasSummary?: GasSummaryType }) {
    const [gas, symbol] = useFormatCoin(gasSummary?.totalGas, GAS_TYPE_ARG);

    if (!gasSummary) return null;

    return (
        <Card heading="Gas Fees">
            <div className="flex flex-col items-center gap-1 w-full">
                <div className="flex w-full justify-between">
                    <Text color="steel-dark" variant="bodySmall">
                        You Paid
                    </Text>
                    <Text color="steel-darker" variant="pBody" weight="medium">
                        {gasSummary?.isSponsored ? '0' : gas} {symbol}
                    </Text>
                </div>
                {gasSummary?.isSponsored && gasSummary.owner && (
                    <>
                        <div className="flex w-full justify-between">
                            <Text color="steel-dark" variant="bodySmall">
                                Paid by Sponsor
                            </Text>
                            <Text
                                color="steel-darker"
                                variant="pBody"
                                weight="medium"
                            >
                                {gas} {symbol}
                            </Text>
                        </div>
                        <div className="flex w-full justify-between">
                            <Text color="steel-dark" variant="bodySmall">
                                Sponsor
                            </Text>
                            <ExplorerLink
                                type={ExplorerLinkType.address}
                                address={gasSummary.owner}
                                className="text-hero-dark no-underline"
                            >
                                <Text variant="pBodySmall" truncate mono>
                                    {formatAddress(gasSummary.owner)}
                                </Text>
                            </ExplorerLink>
                        </div>
                    </>
                )}
            </div>
        </Card>
    );
}
