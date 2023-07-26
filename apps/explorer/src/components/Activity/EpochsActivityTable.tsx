// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { ArrowRight12 } from '@mysten/icons';
import { Text } from '@mysten/ui';
import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';

import { genTableDataFromEpochsData } from './utils';
import { useGetEpochs } from '~/hooks/useGetEpochs';
import { Link } from '~/ui/Link';
import { Pagination, useCursorPagination } from '~/ui/Pagination';
import { PlaceholderTable } from '~/ui/PlaceholderTable';
import { TableCard } from '~/ui/TableCard';
import { numberSuffix } from '~/utils/numberUtil';

const DEFAULT_EPOCHS_LIMIT = 20;

interface Props {
	disablePagination?: boolean;
	refetchInterval?: number;
	initialLimit?: number;
}

export function EpochsActivityTable({
	disablePagination,
	initialLimit = DEFAULT_EPOCHS_LIMIT,
}: Props) {
	const [limit, setLimit] = useState(initialLimit);
	const rpc = useRpcClient();

	const { data: count } = useQuery({
		queryKey: ['epochs', 'current'],
		queryFn: async () => rpc.getCurrentEpoch(),
		select: (epoch) => Number(epoch.epoch) + 1,
	});

	const epochs = useGetEpochs(limit);
	const { data, isFetching, pagination, isLoading, isError } = useCursorPagination(epochs);

	const cardData = data ? genTableDataFromEpochsData(data) : undefined;

	return (
		<div className="flex flex-col space-y-3 text-left xl:pr-10">
			{isError && (
				<div className="pt-2 font-sans font-semibold text-issue-dark">Failed to load Epochs</div>
			)}
			{isLoading || isFetching || !cardData ? (
				<PlaceholderTable
					rowCount={limit}
					rowHeight="16px"
					colHeadings={[
						'Epoch',
						'Transaction Blocks',
						'Stake Rewards',
						'Checkpoint Set',
						'Storage Net Inflow',
						'Epoch End',
					]}
					colWidths={['100px', '120px', '40px', '204px', '90px', '38px']}
				/>
			) : (
				<div>
					<TableCard data={cardData.data} columns={cardData.columns} />
				</div>
			)}

			<div className="flex justify-between">
				{!disablePagination ? (
					<Pagination {...pagination} />
				) : (
					<Link to="/recent?tab=epochs" after={<ArrowRight12 className="h-3 w-3 -rotate-45" />}>
						View all
					</Link>
				)}

				<div className="flex items-center space-x-3">
					<Text variant="body/medium" color="steel-dark">
						{count ? numberSuffix(Number(count)) : '-'}
						{` Total`}
					</Text>
					{!disablePagination && (
						<select
							className="form-select rounded-md border border-gray-45 px-3 py-2 pr-8 text-bodySmall font-medium leading-[1.2] text-steel-dark shadow-button"
							value={limit}
							onChange={(e) => {
								setLimit(Number(e.target.value));
								pagination.onFirst();
							}}
						>
							<option value={20}>20 Per Page</option>
							<option value={40}>40 Per Page</option>
							<option value={60}>60 Per Page</option>
						</select>
					)}
				</div>
			</div>
		</div>
	);
}
