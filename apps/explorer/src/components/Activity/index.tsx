// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';
import toast from 'react-hot-toast';

import { Transactions } from '../transactions';

import { CheckpointsTable } from '~/pages/checkpoints/CheckpointsTable';
import { PlayPause } from '~/ui/PlayPause';
import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';

type Props = {
    initialLimit: number;
    disablePagination?: boolean;
};

const AUTO_REFRESH_ID = 'auto-refresh';
const REFETCH_INTERVAL_SECONDS = 10;
const REFETCH_INTERVAL = REFETCH_INTERVAL_SECONDS * 1000;

export function Activity({ initialLimit, disablePagination }: Props) {
    const [paused, setPaused] = useState(false);

    const handlePauseChange = () => {
        if (paused) {
            toast.success(
                `Auto-refreshing on - every ${REFETCH_INTERVAL_SECONDS} seconds`,
                { id: AUTO_REFRESH_ID }
            );
        } else {
            toast.success('Auto-refresh paused', { id: AUTO_REFRESH_ID });
        }

        setPaused((paused) => !paused);
    };

    const refetchInterval = paused ? undefined : REFETCH_INTERVAL;

    return (
        <div>
            <TabGroup size="lg">
                <div className="relative">
                    <TabList>
                        <Tab>Transactions</Tab>
                        <Tab>Checkpoints</Tab>
                    </TabList>

                    <div className="absolute inset-y-0 right-0 -top-1 text-2xl">
                        <PlayPause
                            paused={paused}
                            onChange={handlePauseChange}
                        />
                    </div>
                </div>
                <TabPanels>
                    <TabPanel>
                        <Transactions
                            refetchInterval={refetchInterval}
                            initialLimit={initialLimit}
                            disablePagination={disablePagination}
                        />
                    </TabPanel>

                    <TabPanel>
                        <CheckpointsTable
                            refetchInterval={refetchInterval}
                            initialLimit={initialLimit}
                            disablePagination={disablePagination}
                        />
                    </TabPanel>
                </TabPanels>
            </TabGroup>
        </div>
    );
}
