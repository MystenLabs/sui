// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import TableCard from '../../components/table/TableCard';
import placeholdertheme from '../../styles/placeholder.module.css';

type DataType = {
    rowCount: number;
    rowHeight: string;
    colHeadings: string[];
    colWidths: string[];
};

export default function PlaceholderTable({
    rowCount,
    rowHeight,
    colHeadings,
    colWidths,
}: DataType) {
    const rowEntry = Object.fromEntries(
        colHeadings.map((header, index) => [
            `a${index}`,
            <div
                key={index}
                className={placeholdertheme.placeholder}
                style={{
                    width: colWidths[index],
                    height: rowHeight,
                }}
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

    return <TableCard tabledata={loadingTable} />;
}
