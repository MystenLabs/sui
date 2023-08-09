// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';

import { TopPackagesTable } from './TopPackagesTable';
import { ErrorBoundary } from '../error-boundary/ErrorBoundary';
import { useEnhancedRpcClient } from '~/hooks/useEnhancedRpc';
import { FilterList } from '~/ui/FilterList';
import { TabHeader } from '~/ui/Tabs';

export type DateFilter = '3D' | '7D' | '30D';
export type ApiDateFilter = 'rank3Days' | 'rank7Days' | 'rank30Days';
export const FILTER_TO_API_FILTER: Record<DateFilter, ApiDateFilter> = {
	'3D': 'rank3Days',
	'7D': 'rank7Days',
	'30D': 'rank30Days',
};

export function TopPackagesCard() {
	const rpc = useEnhancedRpcClient();
	const [selectedFilter, setSelectedFilter] = useState<DateFilter>('3D');

	const { data, isLoading } = useQuery({
		queryKey: ['top-packages', selectedFilter],
		queryFn: async () => rpc.getMoveCallMetrics(),
	});

	const filteredData = data ? data[FILTER_TO_API_FILTER[selectedFilter]] : [];

	return (
		<div className="relative">
			<div className="absolute right-0 mt-1">
				<FilterList
					lessSpacing
					options={['3D', '7D', '30D']}
					value={selectedFilter}
					onChange={(val) => setSelectedFilter(val)}
				/>
			</div>
			<TabHeader
				title="Popular Packages"
				tooltip="Popular packages is recomputed on epoch changes."
			>
				<ErrorBoundary>
					<TopPackagesTable data={filteredData} isLoading={isLoading} />
				</ErrorBoundary>
			</TabHeader>
		</div>
	);
}
