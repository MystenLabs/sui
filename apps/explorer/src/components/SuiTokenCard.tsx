// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatAmount } from '@mysten/core';
import { Sui, Refresh16 } from '@mysten/icons';
import { useState } from 'react';

import { StatsWrapper } from './HomeMetrics/FormattedStatsAmount';

import { useSuiCoinData } from '~/hooks/useSuiCoinData';
import { Card } from '~/ui/Card';
import { Heading } from '~/ui/Heading';
import { Text } from '~/ui/Text';
import { ButtonOrLink } from '~/ui/utils/ButtonOrLink';
import clsx from 'clsx';

export function SuiTokenCard() {
    const { data, isLoading, isFetching, refetch } = useSuiCoinData();
    const [isRefreshButtonHovered, setRefreshButtonHovered] = useState(false);
    const formattedPrice = data
        ? data.currentPrice.toLocaleString('en', {
              style: 'currency',
              currency: 'USD',
          })
        : '--';

    return (
        <Card bg="lightBlue" spacing="lg">
            <div className="flex items-center gap-2 md:max-lg:max-w-[336px]">
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
                        {data?.priceChangePercentageOver24H && (
                            <Heading
                                as="div"
                                variant="heading6/medium"
                                color="issue"
                            >
                                {data.priceChangePercentageOver24H > 0
                                    ? '+'
                                    : null}
                                {data.priceChangePercentageOver24H.toFixed(2)}%
                            </Heading>
                        )}
                    </div>
                    <Text variant="subtitleSmallExtra/medium" color="steel">
                        via CoinGecko
                    </Text>
                </div>
            </div>
            <div className="mt-8 w-full md:max-lg:max-w-[336px]">
                <div className="flex items-end gap-8">
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
                        {formatAmount(data?.totalSupply)}
                    </StatsWrapper>
                </div>
                <div className="-mb-2 -mr-2">
                    <ButtonOrLink
                        onClick={() => refetch()}
                        onMouseEnter={() => setRefreshButtonHovered(true)}
                        onMouseLeave={() => setRefreshButtonHovered(false)}
                        className={clsx('p-2 text-steel hover:text-hero')}
                    >
                        <div className="flex items-center gap-1">
                            {isRefreshButtonHovered && (
                                <Text variant="subtitleSmallExtra/medium">
                                    refresh
                                </Text>
                            )}
                            <Refresh16 />
                        </div>
                    </ButtonOrLink>
                </div>
            </div>
        </Card>
    );
}
