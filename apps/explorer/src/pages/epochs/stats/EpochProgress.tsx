// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatDate } from '@mysten/core';
import clsx from 'clsx';

import { getElapsedTime, useEpochProgress } from '~/pages/epochs/utils';
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

    const elapsedTime =
        !inProgress && start && end ? getElapsedTime(start, end) : undefined;

    return (
        <Card
            bg={inProgress ? 'highlight' : 'default'}
            spacing="lg"
            rounded="2xl"
        >
            <div className="flex flex-col space-y-12">
                <div className={clsx(inProgress ? 'space-y-4' : 'space-y-6')}>
                    <div className="flex flex-col gap-2">
                        <Heading
                            color="steel-darker"
                            variant="heading3/semibold"
                        >
                            {inProgress
                                ? `Epoch ${epoch} in progress`
                                : `Epoch ${epoch}`}
                        </Heading>
                        {elapsedTime && (
                            <Heading
                                variant="heading6/medium"
                                color="steel-darker"
                            >
                                {elapsedTime}
                            </Heading>
                        )}
                    </div>
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
