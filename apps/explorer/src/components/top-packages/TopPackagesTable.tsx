// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { type MoveCallMetric } from '@mysten/sui.js';
import { useMemo } from 'react';

import { ObjectLink } from '~/ui/InternalLink';
import { PlaceholderTable } from '~/ui/PlaceholderTable';
import { TableCard } from '~/ui/TableCard';
import { Text } from '~/ui/Text';

interface TopPackagesTableProps {
    data: MoveCallMetric[];
    isLoading: boolean;
}

export function TopPackagesTable({ data, isLoading }: TopPackagesTableProps) {
    const tableData = useMemo(
        () => ({
            data: data?.map(([item, count]) => ({
                module: (
                    <ObjectLink
                        label={item.module}
                        objectId={`${item.package}?module=${item.module}`}
                    />
                ),
                function: (
                    <Text variant="bodySmall/medium">{item.function}</Text>
                ),
                package: <ObjectLink objectId={item.package} />,
                count: <Text variant="bodySmall/medium">{count}</Text>,
            })),
            columns: [
                {
                    header: 'Function',
                    accessorKey: 'function',
                },
                {
                    header: 'Module',
                    accessorKey: 'module',
                },
                {
                    header: 'Package ID',
                    accessorKey: 'package',
                },
                {
                    header: 'Txns',
                    accessorKey: 'count',
                },
            ],
        }),
        [data]
    );

    if (isLoading) {
        return (
            <PlaceholderTable
                colHeadings={['Module', 'Function', 'Package ID', 'Count']}
                rowCount={10}
                rowHeight="15px"
                colWidths={['100px', '120px', '40px', '204px']}
            />
        );
    }

    return <TableCard data={tableData.data} columns={tableData.columns} />;
}
