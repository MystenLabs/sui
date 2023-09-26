// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowRight12 } from '@mysten/icons';
import {
	type ColumnDef,
	flexRender,
	getCoreRowModel,
	getSortedRowModel,
	type SortingState,
	useReactTable,
} from '@tanstack/react-table';
import clsx from 'clsx';
import { useMemo, useState } from 'react';

export interface TableCardProps<DataType extends object> {
	refetching?: boolean;
	data: DataType[];
	columns: ColumnDef<DataType>[];
	sortTable?: boolean;
	defaultSorting?: SortingState;
	noBorderBottom?: boolean;
}

function AscDescIcon({ sorting }: { sorting: 'asc' | 'desc' }) {
	return (
		<ArrowRight12
			fill="currentColor"
			className={clsx(sorting === 'asc' ? '-rotate-90' : 'rotate-90', ' text-steel-darker')}
		/>
	);
}

export function TableCard<DataType extends object>({
	refetching,
	data,
	columns,
	sortTable,
	defaultSorting,
	noBorderBottom,
}: TableCardProps<DataType>) {
	const [sorting, setSorting] = useState<SortingState>(defaultSorting || []);

	// Use Columns to create a table
	const processedcol = useMemo<ColumnDef<DataType>[]>(
		() =>
			columns.map((column) => ({
				...column,
				// cell renderer for each column from react-table
				// cell should be in the column definition
				//TODO: move cell to column definition
				...(!sortTable && { cell: ({ getValue }) => getValue() }),
			})),
		[columns, sortTable],
	);

	const table = useReactTable({
		data,
		columns: processedcol,
		getCoreRowModel: getCoreRowModel(),
		getSortedRowModel: getSortedRowModel(),
		onSortingChange: setSorting,
		enableSorting: !!sortTable,
		enableSortingRemoval: false,
		initialState: {
			sorting,
		},
		state: {
			sorting,
		},
	});

	return (
		<div
			className={clsx(
				'w-full overflow-x-auto pb-4',
				!noBorderBottom && 'border-b border-gray-45',
				refetching && 'opacity-50',
			)}
		>
			<table className="w-full min-w-max border-collapse text-left">
				<thead>
					{table.getHeaderGroups().map((headerGroup) => (
						<tr key={headerGroup.id}>
							{headerGroup.headers.map(({ id, colSpan, column, isPlaceholder, getContext }) => (
								<th
									key={id}
									colSpan={colSpan}
									scope="col"
									className="h-7.5 text-left text-subtitle font-semibold uppercase text-steel-dark"
									onClick={
										column.columnDef.enableSorting ? column.getToggleSortingHandler() : undefined
									}
								>
									<div
										className={clsx(
											'flex items-center gap-1',
											column.columnDef.enableSorting && 'cursor-pointer text-steel-darker',
										)}
									>
										{isPlaceholder ? null : flexRender(column.columnDef.header, getContext())}

										{column.getIsSorted() && (
											<AscDescIcon sorting={column.getIsSorted() as 'asc' | 'desc'} />
										)}
									</div>
								</th>
							))}
						</tr>
					))}
				</thead>
				<tbody>
					{table.getRowModel().rows.map((row) => (
						<tr key={row.id} className="group">
							{row.getVisibleCells().map(({ column, id, getContext }) => (
								<td
									key={id}
									className="h-7.5 text-body text-gray-75 group-hover:bg-gray-40 group-hover:text-gray-90 group-hover:first:rounded-l group-hover:last:rounded-r"
								>
									{flexRender(column.columnDef.cell, getContext())}
								</td>
							))}
						</tr>
					))}
				</tbody>
			</table>
		</div>
	);
}
