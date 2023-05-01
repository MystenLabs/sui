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
    const formattedPrice = data
        ? data.currentPrice.toLocaleString('en', {
              style: 'currency',
              currency: 'USD',
          })
        : '--';

    return (
        <Card bg="lightBlue" spacing="lg">
            <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
                <div className="flex items-center gap-2">
                    <div className="h-4.5 w-4.5 items-center justify-center rounded-full bg-sui p-1">
                        <Sui className="h-full w-full text-white" />
                    </div>
                    <Heading
                        as="div"
                        variant="heading4/semibold"
                        color="steel-darker"
                    >
                        1 SUI = {formattedPrice}
                    </Heading>
                    {data?.priceChangePercentageOver24H && (
                        <Heading
                            as="div"
                            variant="heading6/medium"
                            color="issue"
                        >
                            {data.priceChangePercentageOver24H > 0 ? '+' : null}
                            {data.priceChangePercentageOver24H.toFixed(2)}%
                        </Heading>
                    )}
                </div>
                <div className="sm:ml-auto">
                    <Text variant="subtitleSmallExtra/medium" color="steel">
                        via CoinGecko
                    </Text>
                </div>
            </div>
            <div className="mt-8 flex gap-8">
                <StatsWrapper
                    label="Market Cap"
                    size="sm"
                    postfix="USD"
                    unavailable={isLoading}
                >
                    {formatAmount(data?.fullyDilutedMarketCap)}
                </StatsWrapper>
                <StatsWrapper
                    label="Total Supply"
                    size="sm"
                    postfix="SUI"
                    unavailable={isLoading}
                >
                    10 B
                </StatsWrapper>
            </div>
        </Card>
    );
}
