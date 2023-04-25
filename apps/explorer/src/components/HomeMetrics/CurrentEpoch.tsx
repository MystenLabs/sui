// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatDate, formatAmountParts } from '@mysten/core';
import { format, isToday, isYesterday } from 'date-fns';
import { useMemo } from 'react';

import { NetworkStats } from './NetworkStats';

import { useEpochProgress } from '~/pages/epochs/utils';
import { Heading } from '~/ui/Heading';
import { ProgressBar } from '~/ui/ProgressBar';
import { Text } from '~/ui/Text';

export function CurrentEpoch() {
    const { epoch, progress, label, end, start } = useEpochProgress();

    const formattedDateString = useMemo(() => {
        let formattedDate = '';
        const epochStartDate = new Date(start);
        if (isToday(epochStartDate)) {
            formattedDate = 'Today';
        } else if (isYesterday(epochStartDate)) {
            formattedDate = 'Yesterday';
        } else {
            formattedDate = format(epochStartDate, 'PPP');
        }
        const formattedTime = format(epochStartDate, 'p');
        return `${formattedTime}, ${formattedDate}`;
    }, [start]);

    return (
        <NetworkStats bg="highlight" spacing="none">
            <div className="flex flex-col gap-4">
                <div className="space-y-4">
                    <div className="flex flex-col gap-2">
                        <Heading
                            color="success-dark"
                            variant="heading4/semibold"
                        >
                            Current Epoch
                        </Heading>
                        <Heading
                            color="success-dark"
                            variant="heading4/semibold"
                        >
                            {formatAmountParts(epoch)}
                        </Heading>
                    </div>

                    {!progress && end ? (
                        <div>
                            <Text
                                variant="pSubtitleSmall/normal"
                                uppercase
                                color="steel-darker"
                            >
                                End
                            </Text>
                            <Text
                                variant="pSubtitle/semibold"
                                color="steel-darker"
                            >
                                {formatDate(end)}
                            </Text>
                        </div>
                    ) : null}
                </div>
                {progress ? (
                    <div className="space-y-1.5">
                        <Heading variant="heading6/medium" color="steel-darker">
                            {label}
                        </Heading>
                        <ProgressBar progress={progress} />
                    </div>
                ) : null}
                <div>
                    <Text
                        variant="pSubtitleSmall/semibold"
                        uppercase
                        color="steel"
                    >
                        Started
                    </Text>

                    <Text variant="pSubtitle/semibold" color="steel">
                        {formattedDateString}
                    </Text>
                </div>
            </div>
        </NetworkStats>
    );
}
