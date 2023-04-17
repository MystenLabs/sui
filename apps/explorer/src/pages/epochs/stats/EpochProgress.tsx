// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatDate } from '@mysten/core';

import { useEpochProgress } from '../useEpochProgress';

import { Card } from '~/ui/Card';
import { Heading } from '~/ui/Heading';
import { ProgressBar } from '~/ui/ProgressBar';
import { Text } from '~/ui/Text';

export interface EpochProgressProps {
    epoch?: string;
    start: number;
    end?: number;
    inProgress?: boolean;
}

export function EpochProgress({
    epoch,
    start,
    end,
    inProgress,
}: EpochProgressProps) {
    const { progress, label } = useEpochProgress();

    return (
        <Card bg={inProgress ? 'highlight' : 'default'} spacing="lg">
            <div className="flex min-w-[136px] flex-col space-y-16">
                <div className="space-y-4">
                    <Heading color="steel-darker" variant="heading3/semibold">
                        {inProgress
                            ? `Epoch ${epoch} in progress`
                            : `Epoch ${epoch}`}
                    </Heading>
                    <div>
                        <Text
                            variant="pSubtitleSmall/normal"
                            uppercase
                            color="steel-darker"
                        >
                            Start
                        </Text>
                        <Text variant="pSubtitle/semibold" color="steel-darker">
                            {formatDate(start)}
                        </Text>
                    </div>
                    {!inProgress && end ? (
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
                {inProgress ? (
                    <div className="space-y-1.5">
                        <Heading variant="heading6/medium" color="steel-darker">
                            {label}
                        </Heading>
                        <ProgressBar progress={progress} />
                    </div>
                ) : null}
            </div>
        </Card>
    );
}
