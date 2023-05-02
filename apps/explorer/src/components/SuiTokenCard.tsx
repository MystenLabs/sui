// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatAmount } from '@mysten/core';
import { Sui, Refresh16 } from '@mysten/icons';
import clsx from 'clsx';
import { useState } from 'react';

import { StatsWrapper } from './HomeMetrics/FormattedStatsAmount';

import { useSuiCoinData } from '~/hooks/useSuiCoinData';
import { Card } from '~/ui/Card';
import { Heading } from '~/ui/Heading';
import { Text } from '~/ui/Text';
import { ButtonOrLink } from '~/ui/utils/ButtonOrLink';

export function SuiTokenCard() {
    const { data, isLoading, isFetching, refetch } = useSuiCoinData();
    const {
        priceChangePercentageOver24H,
        currentPrice,
        totalSupply,
        fullyDilutedMarketCap,
    } = data || {};
    const [isRefreshButtonHovered, setRefreshButtonHovered] = useState(false);

    const isPriceChangePositive = Number(priceChangePercentageOver24H) > 0;
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
                            {priceChangePercentageOver24H && (
                                <Heading
                                    as="div"
                                    variant="heading6/medium"
                                    color={
                                        isPriceChangePositive
                                            ? 'success'
                                            : 'issue'
                                    }
                                >
                                    {isPriceChangePositive ? '+' : null}
                                    {priceChangePercentageOver24H.toFixed(2)}%
                                </Heading>
                            )}
                        </div>
                        <Text variant="subtitleSmallExtra/medium" color="steel">
                            via CoinGecko
                        </Text>
                    </div>
                </div>
                <div className="mt-8 w-full">
                    <div className="flex items-end justify-between gap-8">
                        <StatsWrapper
                            label="Market Cap"
                            size="sm"
                            postfix="USD"
                            unavailable={isLoading}
                        >
                            {formatAmount(fullyDilutedMarketCap)}
                        </StatsWrapper>
                        <StatsWrapper
                            label="Total Supply"
                            size="sm"
                            postfix="SUI"
                            unavailable={isLoading}
                        >
                            {formatAmount(totalSupply)}
                        </StatsWrapper>
                        <div className="-mb-2 -mr-2">
                            <ButtonOrLink
                                onClick={() => refetch()}
                                onMouseEnter={() =>
                                    setRefreshButtonHovered(true)
                                }
                                onMouseLeave={() =>
                                    setRefreshButtonHovered(false)
                                }
                                className={clsx(
                                    'p-2 text-steel hover:text-hero'
                                )}
                            >
                                <div className="group flex items-center gap-1">
                                    <div className="opacity-0 transition-opacity duration-100 group-hover:opacity-100">
                                        <Text variant="subtitleSmallExtra/medium">
                                            {isFetching ? 'refreshing' : 'refresh'}
                                        </Text>
                                    </div>
                                    <Refresh16 />
                                </div>
                            </ButtonOrLink>
                        </div>
                    </div>
                </div>
            </div>
        </Card>
    );
}
