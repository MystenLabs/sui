// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCoinMetadata, useFormatCoin, useGetTimeBeforeEpochNumber } from '@mysten/core';
import { Field, Form, useFormikContext } from 'formik';
import { memo, useCallback, useMemo } from 'react';

import { type FormValues } from './StakingCard';
import { createStakeTransaction } from './utils/transaction';
import { parseAmount } from '../../helpers';
import { useTransactionGasBudget, useActiveAddress } from '../../hooks';
import { Card } from '_app/shared/card';
import { Text } from '_app/shared/text';
import NumberInput from '_components/number-input';
import {
	NUM_OF_EPOCH_BEFORE_STAKING_REWARDS_REDEEMABLE,
	NUM_OF_EPOCH_BEFORE_STAKING_REWARDS_STARTS,
} from '_src/shared/constants';
import { CountDownTimer } from '_src/ui/app/shared/countdown-timer';

const HIDE_MAX = true;

export type StakeFromProps = {
	validatorAddress: string;
	coinBalance: bigint;
	coinType: string;
	epoch?: string | number;
};

function StakeForm({ validatorAddress, coinBalance, coinType, epoch }: StakeFromProps) {
	const { values, setFieldValue } = useFormikContext<FormValues>();

	const { data: metadata } = useCoinMetadata(coinType);
	const decimals = metadata?.decimals ?? 0;
	const [maxToken, symbol, queryResult] = useFormatCoin(coinBalance, coinType);

	const transaction = useMemo(() => {
		if (!values.amount || !decimals) return null;
		const amountWithoutDecimals = parseAmount(values.amount, decimals);
		return createStakeTransaction(amountWithoutDecimals, validatorAddress);
	}, [values.amount, validatorAddress, decimals]);

	const activeAddress = useActiveAddress();
	const { data: gasBudget } = useTransactionGasBudget(activeAddress, transaction);

	const setMaxToken = useCallback(() => {
		if (!maxToken) return;
		setFieldValue('amount', maxToken);
	}, [maxToken, setFieldValue]);

	// Reward will be available after 2 epochs
	const startEarningRewardsEpoch = Number(epoch || 0) + NUM_OF_EPOCH_BEFORE_STAKING_REWARDS_STARTS;

	const redeemableRewardsEpoch =
		Number(epoch || 0) + NUM_OF_EPOCH_BEFORE_STAKING_REWARDS_REDEEMABLE;

	const { data: timeBeforeStakeRewardsStarts } =
		useGetTimeBeforeEpochNumber(startEarningRewardsEpoch);

	const { data: timeBeforeStakeRewardsRedeemable } =
		useGetTimeBeforeEpochNumber(redeemableRewardsEpoch);

	return (
		<Form className="flex flex-1 flex-col flex-nowrap items-center" autoComplete="off">
			<div className="flex flex-col justify-between items-center mb-3 mt-3.5 w-full gap-1.5">
				<Text variant="caption" color="gray-85" weight="semibold">
					Enter the amount of SUI to stake
				</Text>
				<Text variant="bodySmall" color="steel" weight="medium">
					Available - {maxToken} {symbol}
				</Text>
			</div>
			<Card
				variant="gray"
				titleDivider
				header={
					<div className="p-2.5 w-full flex bg-white">
						<Field
							data-testid="stake-amount-input"
							component={NumberInput}
							allowNegative={false}
							name="amount"
							className="w-full border-none text-hero-dark text-heading4 font-semibold bg-white placeholder:text-gray-70 placeholder:font-semibold"
							decimals
							suffix={` ${symbol}`}
							autoFocus
						/>
						{!HIDE_MAX ? (
							<button
								className="bg-white border border-solid border-gray-60 hover:border-steel-dark rounded-2xl h-6 w-11 flex justify-center items-center cursor-pointer text-steel-darker hover:text-steel-darker text-bodySmall font-medium disabled:opacity-50 disabled:cursor-auto"
								onClick={setMaxToken}
								disabled={queryResult.isLoading}
								type="button"
							>
								Max
							</button>
						) : null}
					</div>
				}
				footer={
					<div className="py-px flex justify-between w-full">
						<Text variant="body" weight="medium" color="steel-darker">
							Gas Fees
						</Text>
						<Text variant="body" weight="medium" color="steel-darker">
							{gasBudget} {symbol}
						</Text>
					</div>
				}
			>
				<div className="pb-3.75 flex justify-between w-full">
					<Text variant="body" weight="medium" color="steel-darker">
						Staking Rewards Start
					</Text>
					{timeBeforeStakeRewardsStarts > 0 ? (
						<CountDownTimer
							timestamp={timeBeforeStakeRewardsStarts}
							variant="body"
							color="steel-darker"
							weight="semibold"
							label="in"
							endLabel="--"
						/>
					) : (
						<Text variant="body" weight="medium" color="steel-darker">
							{epoch ? `Epoch #${Number(startEarningRewardsEpoch)}` : '--'}
						</Text>
					)}
				</div>
				<div className="pb-3.75 flex justify-between item-center w-full">
					<div className="flex-1">
						<Text variant="pBody" weight="medium" color="steel-darker">
							Staking Rewards Redeemable
						</Text>
					</div>
					<div className="flex-1 flex justify-end gap-1 items-center">
						{timeBeforeStakeRewardsRedeemable > 0 ? (
							<CountDownTimer
								timestamp={timeBeforeStakeRewardsRedeemable}
								variant="body"
								color="steel-darker"
								weight="semibold"
								label="in"
								endLabel="--"
							/>
						) : (
							<Text variant="body" weight="medium" color="steel-darker">
								{epoch ? `Epoch #${Number(redeemableRewardsEpoch)}` : '--'}
							</Text>
						)}
					</div>
				</div>
			</Card>
		</Form>
	);
}

export default memo(StakeForm);
