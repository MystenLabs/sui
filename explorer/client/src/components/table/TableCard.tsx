// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import {
    flexRender,
    getCoreRowModel,
    useReactTable,
} from '@tanstack/react-table';
import { useMemo } from 'react';

import { ReactComponent as ContentFailedStatus } from '../../assets/SVGIcons/failed.svg';
import { ReactComponent as ContentForwardArrow } from '../../assets/SVGIcons/forward-arrow.svg';
import { ReactComponent as ContentSuccessStatus } from '../../assets/SVGIcons/success.svg';
import Longtext from '../../components/longtext/Longtext';

import type { ExecutionStatusType, TransactionKindName } from '@mysten/sui.js';

import styles from './TableCard.module.css';

type Link = {
    url: string;
    name?: string;
    copy?: boolean;
    category?: string;
    isLink?: boolean;
};

type TableColumn = {
    headerLabel: string;
    accessorKey: string;
};
// TODO: update state to use Tuple type
// type Links = [Link, Link?];
type Links = Link[];

// support multiple types with sepcial handling for 'addresses'/links and status
type TxType = {
    [key: string]:
        | string
        | number
        | boolean
        | Links
        | {
              txTypeName: TransactionKindName | undefined;
              status: ExecutionStatusType;
          };
};

function TableCard({
    tabledata,
}: {
    tabledata: {
        data: TxType[];
        columns: TableColumn[];
    };
}) {
    // Make some columns!
    const data = useMemo(() => tabledata.data, [tabledata.data]);

    const columns = useMemo(
        () =>
            tabledata.columns.map((column) => ({
                accessorKey: column.accessorKey,
                id: column.accessorKey,
                cell: (info: any) => {
                    const content = info.getValue();
                    // default types
                    if (
                        typeof content === 'string' ||
                        typeof content === 'number' ||
                        typeof content === 'boolean'
                    ) {
                        return info.getValue();
                    }

                    // handle multple links in one cell
                    if (Array.isArray(content)) {
                        return (
                            <section className={styles.addresses}>
                                {content.map((itm, idx) => (
                                    <div key={idx + itm.url}>
                                        <Longtext
                                            text={itm.url}
                                            alttext={itm.name}
                                            category={itm.category || 'unknown'}
                                            isLink={itm?.isLink}
                                            isCopyButton={itm?.copy}
                                        />
                                        {idx !== content.length - 1 && (
                                            <ContentForwardArrow />
                                        )}
                                    </div>
                                ))}
                            </section>
                        );
                    }
                    // Special handling for status
                    if (typeof content === 'object' && content !== null) {
                        return (
                            <>
                                {content.status === 'success' ? (
                                    <ContentSuccessStatus />
                                ) : (
                                    <ContentFailedStatus />
                                )}{' '}
                                {content.txTypeName}
                            </>
                        );
                    }
                    return '';
                },
                header: column.headerLabel,
            })),
        [tabledata.columns]
    );
    const table = useReactTable({
        data,
        columns,
        getCoreRowModel: getCoreRowModel(),
    });

    return (
        <div className={styles.content}>
            <table className={styles.table}>
                <thead>
                    {table.getHeaderGroups().map((headerGroup) => (
                        <tr key={headerGroup.id}>
                            {headerGroup.headers.map((header) => (
                                <th
                                    key={header.id}
                                    colSpan={header.colSpan}
                                    scope="col"
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
                    {table.getRowModel().rows.map((row) => (
                        <tr key={row.id}>
                            {row.getVisibleCells().map((cell) => (
                                <td
                                    key={cell.id}
                                    className={styles.tableSpacing}
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

export default TableCard;
