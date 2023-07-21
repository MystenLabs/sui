// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type CheckpointPage } from '@mysten/sui.js';
import { Text } from '@mysten/ui';

import { TxTimeType } from '../tx-time/TxTimeType';
import { HighlightedTableCol } from '~/components/Table/HighlightedTableCol';
import { CheckpointLink, CheckpointSequenceLink } from '~/ui/InternalLink';

// Generate table data from the checkpoints data
export const genTableDataFromCheckpointsData = (data: CheckpointPage) => ({
	data: data?.data.map((checkpoint) => ({
		digest: (
			<HighlightedTableCol first>
				<CheckpointLink digest={checkpoint.digest} />
			</HighlightedTableCol>
		),
		time: <TxTimeType timestamp={Number(checkpoint.timestampMs)} />,
		sequenceNumber: <CheckpointSequenceLink sequence={checkpoint.sequenceNumber} />,
		transactionBlockCount: (
			<Text variant="bodySmall/medium" color="steel-darker">
				{checkpoint.transactions.length}
			</Text>
		),
	})),
	columns: [
		{
			header: () => 'Digest',
			accessorKey: 'digest',
		},
		{
			header: () => 'Sequence Number',
			accessorKey: 'sequenceNumber',
		},
		{
			header: () => 'Time',
			accessorKey: 'time',
		},
		{
			header: () => 'Transaction Block Count',
			accessorKey: 'transactionBlockCount',
		},
	],
});
