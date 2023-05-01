// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatDate, formatAmountParts } from '@mysten/core';
import { format, isToday, isYesterday } from 'date-fns';
import { useMemo } from 'react';

import { useEpochProgress } from '~/pages/epochs/utils';
import { Card } from '~/ui/Card';
import { Heading } from '~/ui/Heading';
import { ProgressBar } from '~/ui/ProgressBar';
import { Text } from '~/ui/Text';
import { LinkWithQuery } from '~/ui/utils/LinkWithQuery';

export function CurrentEpoch() {
    const { epoch, progress, label, end, start } = useEpochProgress();

    const formattedDateString = useMemo(() => {
        if (!start) {
            return null;
        }

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
        <LinkWithQuery to={`/epoch/${epoch}`}>
            <Card bg="highlight" height="full" spacing="lg">
                <div className="flex w-full flex-col gap-4">
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
                    <div className="space-y-1.5">
                        <Heading variant="heading6/medium" color="steel-darker">
                            {label ?? '--'}
                        </Heading>
                        <ProgressBar progress={progress || 0} />
                    </div>
                    <div>
                        <Text
                            variant="pSubtitleSmall/semibold"
                            uppercase
                            color="steel-dark"
                        >
                            {formattedDateString ? 'Started' : '--'}
                        </Text>

                        <Text variant="pSubtitle/semibold" color="steel-dark">
                            {formattedDateString || '--'}
                        </Text>
                    </div>
                </div>
            </Card>
        </LinkWithQuery>
    );
}
