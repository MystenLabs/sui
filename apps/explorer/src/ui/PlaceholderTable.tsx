// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Placeholder } from '@mysten/ui';
import { useMemo } from 'react';

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
	const rowEntry = useMemo(
		() =>
			Object.fromEntries(
				colHeadings.map((header, index) => [
					`a${index}`,
					<Placeholder key={index} width={colWidths[index]} height={rowHeight} />,
				]),
			),
		[colHeadings, colWidths, rowHeight],
	);

	const loadingTable = useMemo(
		() => ({
			data: new Array(rowCount).fill(rowEntry),
			columns: colHeadings.map((header, index) => ({
				header: header,
				accessorKey: `a${index}`,
			})),
		}),
		[rowCount, rowEntry, colHeadings],
	);

	return <TableCard data={loadingTable.data} columns={loadingTable.columns} />;
}
