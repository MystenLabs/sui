// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatDate, formatAmountParts } from '@mysten/core';

import { NetworkStats } from './NetworkStats';

import { useEpochProgress } from '~/pages/epochs/utils';
import { Heading } from '~/ui/Heading';
import { ProgressBar } from '~/ui/ProgressBar';
import { Text } from '~/ui/Text';

export function CurrentEpoch() {
    const { epoch, progress, label, end, start } = useEpochProgress();
    const now = new Date();
    const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    const yesterday = new Date(today);
    yesterday.setDate(today.getDate() - 1);

    const inputDate = new Date(start);
    const inputDay = new Date(
        inputDate.getFullYear(),
        inputDate.getMonth(),
        inputDate.getDate()
    );

    const isToday = inputDay.getTime() === today.getTime();
    const isYesterday = inputDay.getTime() === yesterday.getTime();

    let dayLabel = '';
    if (isToday) {
        dayLabel = 'Today';
    } else if (isYesterday) {
        dayLabel = 'Yesterday';
    } else {
        dayLabel = inputDate.toLocaleDateString();
    }

    return (
        <NetworkStats bg="highlight" spacing="none">
            <div className="flex flex-col gap-4">
                <div className="space-y-4">
                    <div className="flex flex-col gap-2">
                        <Heading
                            color="success-dark"
                            variant="heading3/semibold"
                        >
                            Current Epoch
                        </Heading>
                        <Heading
                            color="success-dark"
                            variant="heading3/semibold"
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
                        {formatDate(start, ['hour', 'minute'])} {dayLabel}
                    </Text>
                </div>
            </div>
        </NetworkStats>
    );
}
