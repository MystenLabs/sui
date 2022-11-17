// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useQuery } from '@tanstack/react-query';
import { useMemo } from 'react';

import { truncate } from '../../utils/stringUtils';
import { TxTimeType } from '../tx-time/TxTimeType';

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

type PackageTableData = {
    time?: string | JSX.Element;
    packageId?: string | JSX.Element;
    txnDigest?: string | JSX.Element;
    sender?: string | JSX.Element;
};

const transformTable = (events: SuiEvents) => ({
    data: events.map(
        ({
            event,
            timestamp,
            txDigest,
        }: SuiEventEnvelope): PackageTableData => {
            if (!('publish' in event)) return {};
            return {
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
            };
        }
    ),

    columns: [...columns],
});

export function RecentModulesCard() {
    const rpc = useRpc();

    const { data, isLoading, isSuccess, isError } = useQuery(
        ['recentPackage'],
        async () => {
            const recentPublishMod: PaginatedEvents = await rpc.getEvents(
                {
                    EventType: 'Publish',
                },
                null,
                5,
                'descending'
            );

            return recentPublishMod.data;
        },
        {
            staleTime: TRANSACTION_STALE_TIME,
        }
    );

    const tableData = useMemo(
        () => (data ? transformTable(data) : null),
        [data]
    );

    if (isError || (!isLoading && !tableData?.data.length)) {
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
            {isSuccess && tableData && (
                <TableCard data={tableData.data} columns={tableData.columns} />
            )}
        </section>
    );
}
