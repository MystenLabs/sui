// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetTotalTransactionBlocks } from '@mysten/core';

import { FormattedStatsAmount } from './FormattedStatsAmount';

import { useGetAddressMetrics } from '~/hooks/useGetAddressMetrics';
import { useGetNetworkMetrics } from '~/hooks/useGetNetworkMetrics';
import { Card } from '~/ui/Card';
import { Text } from '~/ui/Text';

export function OnTheNetwork() {
	const { data: networkMetrics } = useGetNetworkMetrics();
	const { data: transactionCount } = useGetTotalTransactionBlocks();
	const { data: addressMetrics } = useGetAddressMetrics();

	return (
		<Card bg="lightBlue" spacing="lg" height="full">
			<div className="flex flex-col gap-5 md:flex-row">
				<div className="flex flex-1 flex-col gap-5">
					<div className="flex items-center gap-2">
						<Text color="steel-darker" variant="caption/semibold">
							On the Network
						</Text>
						<hr className="flex-1 border-gray-45" />
					</div>
					<div className="flex flex-shrink-0 flex-col gap-2">
						<FormattedStatsAmount
							orientation="horizontal"
							label="Txn Blocks"
							tooltip="Total transaction blocks counter"
							amount={transactionCount}
							size="sm"
						/>
						<FormattedStatsAmount
							orientation="horizontal"
							label="Objects"
							tooltip="Total objects counter"
							amount={networkMetrics?.totalObjects}
							size="sm"
						/>
						<FormattedStatsAmount
							orientation="horizontal"
							label="Packages"
							tooltip="Total packages counter"
							amount={networkMetrics?.totalPackages}
							size="sm"
						/>
					</div>
				</div>

				<div className="flex flex-1 flex-col gap-5">
					<div className="flex items-center gap-2">
						<Text color="steel-darker" variant="caption/semibold">
							Accounts
						</Text>
						<hr className="flex-1 border-gray-45" />
					</div>
					<div className="flex flex-shrink-0 flex-col gap-2">
						<FormattedStatsAmount
							orientation="horizontal"
							label="Daily Active"
							tooltip="Total daily active addresses"
							amount={addressMetrics?.dailyActiveAddresses}
							size="sm"
						/>
						<FormattedStatsAmount
							orientation="horizontal"
							label="Total Active"
							tooltip="Total active addresses"
							amount={addressMetrics?.cumulativeActiveAddresses}
							size="sm"
						/>
						<FormattedStatsAmount
							orientation="horizontal"
							label="Total"
							tooltip="Addresses that have participated in at least one transaction since network genesis"
							amount={addressMetrics?.cumulativeAddresses}
							size="sm"
						/>
					</div>
				</div>
			</div>
		</Card>
	);
}
