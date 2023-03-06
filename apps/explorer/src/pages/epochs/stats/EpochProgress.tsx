// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useEpochProgress } from '../useEpochProgress';

import { Card } from '~/ui/Card';
import { Heading } from '~/ui/Heading';
import { ProgressBar } from '~/ui/ProgressBar';
import { Text } from '~/ui/Text';
import { formatDate } from '~/utils/timeUtils';

export interface EpochProgressProps {
    epoch?: number;
    start: number;
    end: number;
    inProgress?: boolean;
}

export function EpochProgress({
    epoch,
    start,
    end,
    inProgress = true,
}: EpochProgressProps) {
    const { progress, label } = useEpochProgress(start, end);

    return (
        <Card bg={inProgress ? 'highlight' : 'default'} spacing="lg">
            <div className="flex flex-col space-y-16">
                <div className="space-y-4">
                    <Heading color="steel-darker" variant="heading3/semibold">
                        Epoch {epoch} {inProgress ? 'in progress' : ''}
                    </Heading>
                    <div>
                        <Text variant="p4/normal" color="steel-darker">
                            START
                        </Text>
                        <Text variant="p3/semibold" color="steel-darker">
                            {formatDate(start)}
                        </Text>
                    </div>
                </div>

                <div className="space-y-1.5">
                    <Heading variant="heading6/medium" color="steel-darker">
                        {label}
                    </Heading>
                    <ProgressBar progress={progress} />
                </div>
            </div>
        </Card>
    );
}
