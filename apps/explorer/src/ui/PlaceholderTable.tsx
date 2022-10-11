// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import TableCard from './TableCard';

import placeholdertheme from './placeholder.module.css';

type DataType = {
    rowCount: number;
    rowHeight: string;
    colHeadings: string[];
    colWidths: string[];
};

export function PlaceholderBox({
    width,
    height,
}: {
    width: string;
    height: string;
}) {
    return (
        <div
            className={placeholdertheme.placeholder}
            style={{
                width,
                height,
            }}
        />
    );
}

export default function PlaceholderTable({
    rowCount,
    rowHeight,
    colHeadings,
    colWidths,
}: DataType) {
    const rowEntry = Object.fromEntries(
        colHeadings.map((header, index) => [
            `a${index}`,
            <PlaceholderBox
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
        <div className={placeholdertheme.container}>
            <TableCard tabledata={loadingTable} />
        </div>
    );
}
