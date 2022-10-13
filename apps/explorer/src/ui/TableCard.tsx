// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import {
    flexRender,
    getCoreRowModel,
    useReactTable,
} from '@tanstack/react-table';
import { cva } from 'class-variance-authority';
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

export type TableType = {
    data: DataType[];
    columns: TableColumn[];
};

const cellStyle = cva(['text-sui-grey-75 h-[30px] px-[4px]'], {
    variants: {
        variant: {
            th: 'text-left font-semibold uppercase text-xs',
            td: 'group-hover:text-sui-grey-90 group-hover:bg-sui-grey-40 text-sm group-hover:first:rounded-l-[4px] group-hover:last:rounded-r-[4px]',
        },
    },
    defaultVariants: {
        variant: 'td',
    },
});

export function TableCard({ tabledata }: { tabledata: TableType }) {
    const data = useMemo(() => tabledata.data, [tabledata.data]);
    // Use Columns to create a table
    const columns = useMemo(
        () => columnsContent(tabledata.columns),
        [tabledata.columns]
    );
    const table = useReactTable({
        data,
        columns,
        getCoreRowModel: getCoreRowModel(),
    });

    return (
        <div className={'w-full overflow-x-auto'}>
            <table
                className={
                    'text-sm text-left min-w-max border-collapse w-full  border-solid border-[#f0f1f2] border-b-[1px] border-0'
                }
            >
                <thead>
                    {table.getHeaderGroups().map((headerGroup: any) => (
                        <tr key={headerGroup.id}>
                            {headerGroup.headers.map((header: any) => (
                                <th
                                    key={header.id}
                                    colSpan={header.colSpan}
                                    scope="col"
                                    className={cellStyle({ variant: 'th' })}
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
                        <tr key={row.id} className={'group'}>
                            {row.getVisibleCells().map((cell: any) => (
                                <td
                                    key={cell.id}
                                    className={cellStyle({ variant: 'td' })}
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
