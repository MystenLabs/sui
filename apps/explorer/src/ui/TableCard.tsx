// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    flexRender,
    getCoreRowModel,
    useReactTable,
} from '@tanstack/react-table';
import clsx from 'clsx';
import { useMemo } from 'react';

import type { ExecutionStatusType, TransactionKindName } from '@mysten/sui.js';

export type LinkObj = {
    url: string;
    name?: string;
    copy?: boolean;
    category?: string;
    isLink?: boolean;
};

type TableColumn = {
    headerLabel: string | (() => JSX.Element);
    accessorKey: string;
};
// TODO: update Link to use Tuple type
// type Links = [Link, Link?];
type Links = LinkObj[];

type TxStatus = {
    txTypeName: TransactionKindName | undefined;
    status: ExecutionStatusType;
};

// support multiple types with special handling for 'addresses'/links and status
// TODO: Not sure to allow HTML elements in the table
type DataType = {
    [key: string]:
        | string
        | number
        | boolean
        | Links
        | React.ReactElement
        | TxStatus;
};

function columnsContent(columns: TableColumn[]) {
    return columns.map((column) => ({
        accessorKey: column.accessorKey,
        id: column.accessorKey,
        header: column.headerLabel,
        // cell renderer for each column from react-table
        cell: (info: any) => info.getValue(),
    }));
}

export interface TableCardProps {
    refetching?: boolean;
    data: DataType[];
    columns: TableColumn[];
}

export function TableCard({ refetching, data, columns }: TableCardProps) {
    // Use Columns to create a table
    const processedcol = useMemo(() => columnsContent(columns), [columns]);
    const table = useReactTable({
        data,
        columns: processedcol,
        getCoreRowModel: getCoreRowModel(),
    });

    return (
        <div
            className={clsx(
                'w-full overflow-x-auto border-solid border-0 border-gray-45 border-b pb-4',
                refetching && 'opacity-50'
            )}
        >
            <table className="text-left min-w-max border-collapse w-full border-0">
                <thead>
                    {table.getHeaderGroups().map((headerGroup: any) => (
                        <tr key={headerGroup.id}>
                            {headerGroup.headers.map((header: any) => (
                                <th
                                    key={header.id}
                                    colSpan={header.colSpan}
                                    scope="col"
                                    className="text-gray-75 h-[30px] px-1 text-left font-semibold uppercase text-subtitle"
                                >
                                    {header.isPlaceholder
                                        ? null
                                        : flexRender(
                                              header.column.columnDef.header,
                                              header.getContext()
                                          )}
                                </th>
                            ))}
                        </tr>
                    ))}
                </thead>
                <tbody>
                    {table.getRowModel().rows.map((row: any) => (
                        <tr key={row.id} className="group">
                            {row.getVisibleCells().map((cell: any) => (
                                <td
                                    key={cell.id}
                                    className="text-gray-75 h-[30px] px-1 group-hover:text-gray-90 group-hover:bg-gray-40 text-body group-hover:first:rounded-l group-hover:last:rounded-r"
                                >
                                    {flexRender(
                                        cell.column.columnDef.cell,
                                        cell.getContext()
                                    )}
                                </td>
                            ))}
                        </tr>
                    ))}
                </tbody>
            </table>
        </div>
    );
}
