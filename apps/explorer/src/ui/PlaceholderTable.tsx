// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Placeholder } from './Placeholder';
import { TableCard } from './TableCard';

export interface PlaceholderTableProps {
    rowCount: number;
    rowHeight: string;
    colHeadings: string[];
    colWidths: string[];
}

export function PlaceholderTable({
    rowCount,
    rowHeight,
    colHeadings,
    colWidths,
}: PlaceholderTableProps) {
    const rowEntry = Object.fromEntries(
        colHeadings.map((header, index) => [
            `a${index}`,
            <Placeholder
                key={index}
                width={colWidths[index]}
                height={rowHeight}
            />,
        ])
    );

    const loadingTable = {
        data: new Array(rowCount).fill(rowEntry),
        columns: colHeadings.map((header, index) => ({
            headerLabel: header,
            accessorKey: `a${index}`,
        })),
    };

    return (
        <TableCard data={loadingTable.data} columns={loadingTable.columns} />
    );
}
