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

type Category =
    | 'objects'
    | 'transactions'
    | 'addresses'
    | 'ethAddress'
    | 'unknown';

export type Link = {
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
// TODO: update Link to use Tuple type
// type Links = [Link, Link?];
type Links = Link[];

type TxStatus = {
    txTypeName: TransactionKindName | undefined;
    status: ExecutionStatusType;
};

// support multiple types with special handling for 'addresses'/links and status
// TODO: Not sure to allow HTML elements in the table
type TxType = {
    [key: string]:
        | string
        | number
        | boolean
        | Links
        | React.ReactElement
        | TxStatus;
};

export function TxAddresses({ content }: { content: Link[] }) {
    return (
        <section className={styles.addresses}>
            {content.map((itm, idx) => (
                <div key={idx + itm.url} className={styles.longtextwrapper}>
                    <Longtext
                        text={itm.url}
                        alttext={itm.name}
                        category={(itm.category as Category) || 'unknown'}
                        isLink={itm?.isLink}
                        isCopyButton={itm?.copy}
                    />
                    {idx !== content.length - 1 && <ContentForwardArrow />}
                </div>
            ))}
        </section>
    );
}

function TxStatusType({ content }: { content: TxStatus }) {
    const TxStatus = {
        success: ContentSuccessStatus,
        fail: ContentFailedStatus,
    };
    const TxResultStatus =
        content.status === 'success' ? TxStatus.success : TxStatus.fail;
    return (
        <>
            <TxResultStatus /> {content.txTypeName}
        </>
    );
}

function columnsContent(columns: TableColumn[]) {
    return columns.map((column) => ({
        accessorKey: column.accessorKey,
        id: column.accessorKey,
        header: column.headerLabel,
        // cell renderer for each column from react-table
        cell: (info: any) => {
            const content = info.getValue();

            // handle multiple links in one cell
            if (Array.isArray(content)) {
                return <TxAddresses content={content} />;
            }
            // Special case for txTypes and status
            if (
                typeof content === 'object' &&
                content !== null &&
                content.txTypeName
            ) {
                return <TxStatusType content={content} />;
            }
            // handle most common types
            return content;
        },
    }));
}

function TableCard({
    tabledata,
}: {
    tabledata: {
        data: TxType[];
        columns: TableColumn[];
    };
}) {
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
        <div className={styles.content}>
            <table className={styles.table}>
                <thead>
                    {table.getHeaderGroups().map((headerGroup: any) => (
                        <tr key={headerGroup.id}>
                            {headerGroup.headers.map((header: any) => (
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
                    {table.getRowModel().rows.map((row: any) => (
                        <tr key={row.id}>
                            {row.getVisibleCells().map((cell: any) => (
                                <td
                                    key={cell.id}
                                    className={styles.tablespacing}
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
