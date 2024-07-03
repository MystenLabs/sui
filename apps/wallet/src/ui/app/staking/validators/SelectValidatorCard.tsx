// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Content, Menu } from '_app/shared/bottom-menu-layout';
import { Button } from '_app/shared/ButtonUI';
import { Text } from '_app/shared/text';
import Alert from '_components/alert';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { ampli } from '_src/shared/analytics/ampli';
import { calculateStakeShare, formatPercentageDisplay, useGetValidatorsApy } from '@mysten/core';
import { useSuiClientQuery } from '@mysten/dapp-kit';
import { ArrowRight16 } from '@mysten/icons';
import cl from 'clsx';
import { useMemo, useState } from 'react';

import { ValidatorListItem } from './ValidatorListItem';

type SortKeys = 'name' | 'stakeShare' | 'apy';
const sortKeys: Record<SortKeys, string> = {
	name: 'Name',
	stakeShare: 'Stake Share',
	apy: 'APY',
};

type Validator = {
	name: string;
	address: string;
	apy: number | null;
	isApyApproxZero?: boolean;
	stakeShare: number;
};

export function SelectValidatorCard() {
	const [selectedValidator, setSelectedValidator] = useState<Validator | null>(null);
	const [sortKey, setSortKey] = useState<SortKeys | null>(null);
	const [sortAscending, setSortAscending] = useState(true);
	const { data, isPending, isError } = useSuiClientQuery('getLatestSuiSystemState');

	const { data: rollingAverageApys } = useGetValidatorsApy();

	const selectValidator = (validator: Validator) => {
		setSelectedValidator((state) => (state?.address !== validator.address ? validator : null));
	};

	const handleSortByKey = (key: SortKeys) => {
		if (key === sortKey) {
			setSortAscending(!sortAscending);
		}
		setSortKey(key);
	};

	const totalStake = useMemo(() => {
		if (!data) return 0;
		return data.activeValidators.reduce(
			(acc, curr) => (acc += BigInt(curr.stakingPoolSuiBalance)),
			0n,
		);
	}, [data]);

	const validatorsRandomOrder = useMemo(
		() => [...(data?.activeValidators || [])].sort(() => 0.5 - Math.random()),
		[data?.activeValidators],
	);
	const validatorList = useMemo(() => {
		const sortedAsc = validatorsRandomOrder.map((validator) => {
			const { apy, isApyApproxZero } = rollingAverageApys?.[validator.suiAddress] ?? { apy: null };
			return {
				name: validator.name,
				address: validator.suiAddress,
				apy,
				isApyApproxZero,
				stakeShare: calculateStakeShare(
					BigInt(validator.stakingPoolSuiBalance),
					BigInt(totalStake),
				),
			};
		});
		if (sortKey) {
			sortedAsc.sort((a, b) => {
				if (sortKey === 'name') {
					return a[sortKey].localeCompare(b[sortKey], 'en', {
						sensitivity: 'base',
						numeric: true,
					});
				}
				// since apy can be null, fallback to 0
				return (a[sortKey] || 0) - (b[sortKey] || 0);
			});

			return sortAscending ? sortedAsc : sortedAsc.reverse();
		}
		return sortedAsc;
	}, [validatorsRandomOrder, sortAscending, rollingAverageApys, totalStake, sortKey]);

	if (isPending) {
		return (
			<div className="p-2 w-full flex justify-center items-center h-full">
				<LoadingIndicator />
			</div>
		);
	}

	if (isError) {
		return (
			<div className="p-2">
				<Alert>
					<div className="mb-1 font-semibold">Something went wrong</div>
				</Alert>
			</div>
		);
	}

	return (
		<div className="flex flex-col w-full h-full -my-5">
			<Content className="flex flex-col w-full items-center">
				<div className="flex flex-col w-full items-center -top-5 bg-white sticky pt-5 pb-2.5 z-50 mt-0">
					<div className="flex items-start w-full mb-2">
						<Text variant="subtitle" weight="medium" color="steel-darker">
							Sort by:
						</Text>
						<div className="flex items-center ml-2 gap-1.5">
							{Object.entries(sortKeys).map(([key, value]) => {
								return (
									<button
										key={key}
										className="bg-transparent border-0 p-0 flex gap-1 cursor-pointer"
										onClick={() => handleSortByKey(key as SortKeys)}
									>
										<Text
											variant="caption"
											weight="medium"
											color={sortKey === key ? 'hero' : 'steel-darker'}
										>
											{value}
										</Text>
										{sortKey === key && (
											<ArrowRight16
												className={cl(
													'text-captionSmall font-thin text-hero',
													sortAscending ? 'rotate-90' : '-rotate-90',
												)}
											/>
										)}
									</button>
								);
							})}
						</div>
					</div>
					<div className="flex items-start w-full">
						<Text variant="subtitle" weight="medium" color="steel-darker">
							Select a validator to start staking SUI.
						</Text>
					</div>
				</div>
				<div className="flex items-start flex-col w-full mt-1 flex-1">
					{data &&
						validatorList.map((validator) => (
							<div
								data-testid="validator-list-item"
								className="cursor-pointer w-full relative"
								key={validator.address}
								onClick={() => selectValidator(validator)}
							>
								<ValidatorListItem
									selected={selectedValidator?.address === validator.address}
									validatorAddress={validator.address}
									value={formatPercentageDisplay(
										!sortKey || sortKey === 'name' ? null : validator[sortKey],
										'-',
										validator?.isApyApproxZero,
									)}
								/>
							</div>
						))}
				</div>
			</Content>
			{selectedValidator && (
				<Menu stuckClass="staked-cta" className="w-full px-0 pb-5 mx-0 -bottom-5">
					<Button
						data-testid="select-validator-cta"
						size="tall"
						variant="primary"
						to={`/stake/new?address=${encodeURIComponent(selectedValidator.address)}`}
						onClick={() =>
							ampli.selectedValidator({
								validatorName: selectedValidator.name,
								validatorAddress: selectedValidator.address,
								validatorAPY: selectedValidator.apy || 0,
							})
						}
						text="Select Amount"
						after={<ArrowRight16 />}
					/>
				</Menu>
			)}
		</div>
	);
}
