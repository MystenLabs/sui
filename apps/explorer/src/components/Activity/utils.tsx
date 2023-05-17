// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { type EpochPage } from '@mysten/sui.js/src/types/epochs';

import { SuiAmount } from '../Table/SuiAmount';
import { TxTableCol } from '../transactions/TxCardUtils';
import { TxTimeType } from '../tx-time/TxTimeType';

import { CheckpointSequenceLink, EpochLink } from '~/ui/InternalLink';
import { Text } from '~/ui/Text';
import { getEpochStorageFundFlow } from '~/utils/getStorageFundFlow';

// Generate table data from the epochs data
export const genTableDataFromEpochsData = (results: EpochPage) => ({
    data: results?.data.map((epoch) => ({
        epoch: (
            <TxTableCol isHighlightedOnHover>
                <EpochLink epoch={epoch.epoch.toString()} />
            </TxTableCol>
        ),
        transactions: (
            <TxTableCol>
                <Text variant="bodySmall/medium">
                    {epoch.epochTotalTransactions}
                </Text>
            </TxTableCol>
        ),
        stakeRewards: (
            <TxTableCol>
                <SuiAmount
                    amount={epoch.endOfEpochInfo?.totalStakeRewardsDistributed}
                />
            </TxTableCol>
        ),
        checkpointSet: (
            <div>
                <CheckpointSequenceLink
                    sequence={epoch.firstCheckpointId.toString()}
                />
                {` - `}
                <CheckpointSequenceLink
                    sequence={
                        epoch.endOfEpochInfo?.lastCheckpointId.toString() ?? ''
                    }
                />
            </div>
        ),
        storageNetInflow: (
            <TxTableCol>
                <SuiAmount
                    amount={
                        getEpochStorageFundFlow(epoch.endOfEpochInfo).netInflow
                    }
                />
            </TxTableCol>
        ),
        time: (
            <TxTableCol>
                <TxTimeType
                    timestamp={Number(
                        epoch.endOfEpochInfo?.epochEndTimestamp ?? 0
                    )}
                />
            </TxTableCol>
        ),
    })),
    columns: [
        {
            header: 'Epoch',
            accessorKey: 'epoch',
        },
        {
            header: 'Transaction Blocks',
            accessorKey: 'transactions',
        },
        {
            header: 'Stake Rewards',
            accessorKey: 'stakeRewards',
        },
        {
            header: 'Checkpoint Set',
            accessorKey: 'checkpointSet',
        },
        {
            header: 'Storage Net Inflow',
            accessorKey: 'storageNetInflow',
        },
        {
            header: 'Epoch End',
            accessorKey: 'time',
        },
    ],
});
