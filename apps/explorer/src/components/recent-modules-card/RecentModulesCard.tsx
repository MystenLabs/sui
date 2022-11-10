// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useQuery } from '@tanstack/react-query';

import { TxTimeType } from '../../components/tx-time/TxTimeType';
import { truncate } from '../../utils/stringUtils';

import type {
    SuiEventEnvelope,
    PaginatedEvents,
    SuiEvents,
} from '@mysten/sui.js';

import { useRpc } from '~/hooks/useRpc';
import { Banner } from '~/ui/Banner';
import { Link } from '~/ui/Link';
import { PlaceholderTable } from '~/ui/PlaceholderTable';
import { TableCard } from '~/ui/TableCard';

const TRANSACTION_STALE_TIME = 10 * 1000;
const TRUNCATE_LENGTH = 16;

export function RecentModulesCard() {
    const rpc = useRpc();

    const columns = [
        {
            headerLabel: 'Time',
            accessorKey: 'time',
        },
        {
            headerLabel: 'Package ID',
            accessorKey: 'packageId',
        },
        {
            headerLabel: 'Transaction ID',
            accessorKey: 'txnDigest',
        },
        {
            headerLabel: 'Sender',
            accessorKey: 'sender',
        },
    ];

    const transformTable = (events: SuiEvents) => ({
        data: events.map((resp: SuiEventEnvelope) => {
            const { event, timestamp, txDigest } = resp;
            return {
                ...('publish' in event && {
                    time: <TxTimeType timestamp={timestamp} />,

                    sender: (
                        <Link
                            variant="mono"
                            to={`/addresses/${encodeURIComponent(
                                event.publish.sender
                            )}`}
                        >
                            {truncate(event.publish.sender, TRUNCATE_LENGTH)}
                        </Link>
                    ),
                    packageId: (
                        <Link
                            variant="mono"
                            to={`/objects/${encodeURIComponent(
                                event.publish.packageId
                            )}`}
                        >
                            {truncate(event.publish.packageId, TRUNCATE_LENGTH)}
                        </Link>
                    ),

                    txnDigest: (
                        <Link
                            variant="mono"
                            to={`/transactions/${encodeURIComponent(txDigest)}`}
                        >
                            {truncate(txDigest, TRUNCATE_LENGTH)}
                        </Link>
                    ),
                }),
            };
        }),
        columns: [...columns],
    });

    const { data, isLoading, isSuccess, isError } = useQuery(
        ['normalized-module'],
        async () => {
            const recentPublishMod: PaginatedEvents = await rpc.getEvents(
                {
                    EventType: 'Publish',
                },
                null,
                5,
                'descending'
            );

            return transformTable(recentPublishMod.data);
        },
        {
            staleTime: TRANSACTION_STALE_TIME,
        }
    );

    if (isError) {
        return (
            <Banner variant="error" fullWidth>
                No Package Found
            </Banner>
        );
    }

    return (
        <section>
            {isLoading && (
                <PlaceholderTable
                    rowCount={4}
                    rowHeight="13px"
                    colHeadings={[
                        'Time',
                        'Package ID',
                        'Transaction ID',
                        'Sender',
                    ]}
                    colWidths={['25px', '135px', '220px', '220px']}
                />
            )}
            {isSuccess && (
                <TableCard data={data?.data} columns={data?.columns} />
            )}
        </section>
    );
}
