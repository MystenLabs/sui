// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatAmount } from '@mysten/core';
import { Sui } from '@mysten/icons';

import { StatsWrapper } from './HomeMetrics/FormattedStatsAmount';

import { useSuiCoinData } from '~/hooks/useSuiCoinData';
import { Card } from '~/ui/Card';
import { Heading } from '~/ui/Heading';
import { Text } from '~/ui/Text';

export function SuiTokenCard() {
    const { data, isLoading } = useSuiCoinData();
    const { currentPrice, totalSupply, marketCap } = data || {};

    const formattedPrice = currentPrice
        ? currentPrice.toLocaleString('en', {
              style: 'currency',
              currency: 'USD',
          })
        : '--';

    return (
        <Card bg="lightBlue" spacing="lg">
            <div className="md:max-lg:max-w-[336px]">
                <div className="flex items-center gap-2">
                    <div className="h-4.5 w-4.5 rounded-full bg-sui p-1">
                        <Sui className="h-full w-full text-white" />
                    </div>
                    <div className="flex w-full flex-col gap-1 sm:flex-row sm:items-center sm:justify-between">
                        <div className="flex items-center gap-2">
                            <Heading
                                as="div"
                                variant="heading4/semibold"
                                color="steel-darker"
                            >
                                1 SUI = {formattedPrice}
                            </Heading>
                        </div>
                        <Text variant="subtitleSmallExtra/medium" color="steel">
                            via CoinGecko
                        </Text>
                    </div>
                </div>
                <div className="mt-8 flex w-full gap-8">
                    <StatsWrapper
                        label="Market Cap"
                        size="sm"
                        postfix="USD"
                        unavailable={isLoading}
                    >
                        {formatAmount(marketCap)}
                    </StatsWrapper>
                    <StatsWrapper
                        label="Total Supply"
                        size="sm"
                        postfix="SUI"
                        unavailable={isLoading}
                    >
                        {formatAmount(totalSupply)}
                    </StatsWrapper>
                </div>
            </div>
        </Card>
    );
}
