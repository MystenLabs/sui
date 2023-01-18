// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    flexRender,
    getCoreRowModel,
    getSortedRowModel,
    useReactTable,
    type SortingState,
} from '@tanstack/react-table';
import clsx from 'clsx';
import { useMemo, useState } from 'react';

import { ReactComponent as ArrowRight } from '../assets/SVGIcons/12px/ArrowRight.svg';

import type { ExecutionStatusType, TransactionKindName } from '@mysten/sui.js';

type Category = 'object' | 'transaction' | 'address' | 'unknown';

export type LinkObj = {
    url: string;
    name?: string;
    copy?: boolean;
    category?: Category;
    isLink?: boolean;
};

type TableColumn = {
    headerLabel: string | (() => JSX.Element);
    accessorKey: string;
    sorting?: boolean;
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
        enableSorting:!!column.sorting,
        // cell renderer for each column from react-table
        cell: (info: any) => info.getValue(),
    }));
}

export interface TableCardProps {
    refetching?: boolean;
    data: DataType[];
    columns: TableColumn[];
    enableSorting?: boolean;
}

export function TableCard({ refetching, data, columns, enableSorting }: TableCardProps) {
    // Use Columns to create a table
    const processedcol = useMemo(() => columnsContent(columns), [columns]);
    const [sorting, setSorting] = useState<SortingState>([]);

    const table = useReactTable({
        data,
        columns: processedcol,
        getCoreRowModel: getCoreRowModel(),
        getSortedRowModel: getSortedRowModel(),
        onSortingChange: setSorting,
        enableSorting: !!enableSorting,
        state: {
            sorting,
          },
    },
    );

    return (
        <div
            className={clsx(
                'w-full overflow-x-auto border-0 border-b border-solid border-gray-45 pb-4',
                refetching && 'opacity-50'
            )}
        >
            <table className="w-full min-w-max border-collapse border-0 text-left">
                <thead>
                    {table.getHeaderGroups().map((headerGroup) => (
                        <tr key={headerGroup.id}>
                            {headerGroup.headers.map(({id, colSpan, column, isPlaceholder, getContext }) => (
                                <th
                                    key={id}
                                    colSpan={colSpan}
                                    scope="col"
                                    className="h-7.5 px-1 text-left text-subtitle font-semibold uppercase text-steel-dark"
                                    onClick={column.columnDef.enableSorting ? column.getToggleSortingHandler() : void(0)}
                                   
                                >
                                    <div className="gap-1 items-center flex">
                                
                                    {isPlaceholder
                                        ? null
                                        : flexRender(
                                              column.columnDef.header,
                                              getContext()
                                          )}
                                           {{
                                            asc: <ArrowRight fill="currentColor" className='-rotate-90 text-steel-darker'/>,
                                            desc: <ArrowRight fill="currentColor"  className='rotate-90 text-steel-darker'/>,
                                            }[column.getIsSorted() as string] ?? null}
                                            </div>
                                    
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
                                    className="h-7.5 px-1 text-body text-gray-75 group-hover:bg-gray-40 group-hover:text-gray-90 group-hover:first:rounded-l group-hover:last:rounded-r"
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