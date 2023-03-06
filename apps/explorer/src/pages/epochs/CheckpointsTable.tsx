// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useQuery } from '@tanstack/react-query';

import { getCheckpoints } from './mocks';

import { TxTimeType } from '~/components/tx-time/TxTimeType';
import { TableCard } from '~/ui/TableCard';
import { Text } from '~/ui/Text';

export function CheckpointsTable({ epoch }: { epoch: number }) {
    const { data: checkpointsTable } = useQuery(
        ['checkpoints', epoch],
        async () => {
            const checkpoints = await getCheckpoints();
            return {
                data: checkpoints.map((checkpoint) => ({
                    time: (
                        <Text variant="bodySmall/medium" color="steel-darker">
                            <TxTimeType timestamp={checkpoint.timestampMs} />
                        </Text>
                    ),
                    sequenceNumber: (
                        <Text variant="bodySmall/medium" color="steel-darker">
                            {checkpoint.sequence_number}
                        </Text>
                    ),
                    transactionCount: (
                        <Text variant="bodySmall/medium" color="steel-darker">
                            {checkpoint.transaction_count}
                        </Text>
                    ),
                    digest: (
                        <Text variant="bodySmall/medium" color="steel-darker">
                            {checkpoint.content_digest}
                        </Text>
                    ),
                    signature: (
                        <Text variant="bodySmall/medium" color="steel-darker">
                            {checkpoint.signature}
                        </Text>
                    ),
                })),
                columns: [
                    {
                        header: 'Time',
                        accessorKey: 'time',
                    },
                    {
                        header: 'Sequence Number',
                        accessorKey: 'sequenceNumber',
                    },
                    {
                        header: 'Transaction Count',
                        accessorKey: 'transactionCount',
                    },
                    {
                        header: 'Digest',
                        accessorKey: 'digest',
                    },
                    {
                        header: 'Signature',
                        accessorKey: 'signature',
                    },
                ],
            };
        }
    );

    return checkpointsTable ? (
        <TableCard
            data={checkpointsTable.data}
            columns={checkpointsTable.columns}
        />
    ) : null;
}
