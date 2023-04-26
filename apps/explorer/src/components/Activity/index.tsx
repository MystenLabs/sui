// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';
// import toast from 'react-hot-toast';

import { CheckpointsTable } from '../checkpoints/CheckpointsTable';
import { TransactionsActivityTable } from './TransactionsActivityTable';

import { EpochsTable } from '~/pages/epochs/EpochsTable';
// import { PlayPause } from '~/ui/PlayPause';
import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';

type Props = {
    initialTab?: string | null;
    initialLimit: number;
    disablePagination?: boolean;
};

// const AUTO_REFRESH_ID = 'auto-refresh';
const REFETCH_INTERVAL_SECONDS = 10;
const REFETCH_INTERVAL = REFETCH_INTERVAL_SECONDS * 1000;

const tabs: Record<string, number> = {
    epochs: 1,
    checkpoints: 2,
};

export function Activity({
    initialTab,
    initialLimit,
    disablePagination,
}: Props) {
    const [selectedIndex, setSelectedIndex] = useState(
        initialTab && tabs[initialTab] ? tabs[initialTab] : 0
    );
    const [paused] = useState(false);

    // const handlePauseChange = () => {
    //     if (paused) {
    //         toast.success(
    //             `Auto-refreshing on - every ${REFETCH_INTERVAL_SECONDS} seconds`,
    //             { id: AUTO_REFRESH_ID }
    //         );
    //     } else {
    //         toast.success('Auto-refresh paused', { id: AUTO_REFRESH_ID });
    //     }

    //     setPaused((paused) => !paused);
    // };

    const refetchInterval = paused ? undefined : REFETCH_INTERVAL;

    return (
        <div>
            <TabGroup
                size="lg"
                selectedIndex={selectedIndex}
                onChange={setSelectedIndex}
            >
                <div className="relative">
                    <TabList>
                        <Tab>Transaction Blocks</Tab>
                        <Tab>Epochs</Tab>
                        <Tab>Checkpoints</Tab>
                    </TabList>
                    <div className="absolute inset-y-0 -top-1 right-0 text-2xl">
                        {/* todo: re-enable this when rpc is stable */}
                        {/* <PlayPause
                            paused={paused}
                            onChange={handlePauseChange}
                        /> */}
                    </div>
                </div>
                <TabPanels>
                    <TabPanel>
                        <TransactionsActivityTable
                            refetchInterval={refetchInterval}
                            initialLimit={initialLimit}
                            disablePagination={disablePagination}
                        />
                    </TabPanel>
                    <TabPanel>
                        <EpochsTable
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
