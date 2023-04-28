// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Sui } from '@mysten/icons';

import { StatsWrapper } from './HomeMetrics/FormattedStatsAmount';

import { Card } from '~/ui/Card';
import { Heading } from '~/ui/Heading';

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
                    1 SUI = --
                </Heading>
                {/* <div className="ml-auto">
                    <Text variant="subtitleSmallExtra/medium" color="steel">
                        via CoinMarketCap
                    </Text>
                </div> */}
            </div>
            <div className="mt-8 flex gap-8">
                <StatsWrapper
                    label="Market Cap"
                    size="sm"
                    postfix="USD"
                    unavailable
                />
                <StatsWrapper
                    label="Total Supply"
                    size="sm"
                    postfix="SUI"
                    unavailable
                />
            </div>
        </Card>
    );
}
