// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useActiveAccount } from '_app/hooks/useActiveAccount';
import { Heading } from '_app/shared/heading';
import { Text } from '_app/shared/text';
import { ButtonOrLink } from '_app/shared/utils/ButtonOrLink';
import { CoinIcon } from '_components/coin-icon';
import { DescriptionItem } from '_pages/approval-request/transaction-request/DescriptionList';
import { useGetBalance } from '_pages/swap/utils';
import { useCoinMetadata } from '@mysten/core';
import { ChevronDown16 } from '@mysten/icons';

export function AssetData({
	coinType,
	to,
	onClick,
	disabled,
}: {
	coinType: string;
	to?: string;
	onClick?: () => void;
	disabled?: boolean;
}) {
	const activeAccount = useActiveAccount();
	const currentAddress = activeAccount?.address;

	const { data: balance } = useGetBalance({
		coinType,
		owner: currentAddress,
	});

	const { data: coinMetadata } = useCoinMetadata(coinType);

	return (
		<DescriptionItem
			title={
				<ButtonOrLink
					disabled={disabled}
					onClick={onClick}
					to={to}
					className="flex gap-1 items-center no-underline outline-none border-transparent bg-transparent p-0"
				>
					{!!coinType && <CoinIcon coinType={coinType} size="md" />}
					<Heading variant="heading6" weight="semibold" color="hero-dark">
						{coinMetadata?.symbol || 'Select coin'}
					</Heading>
					{!disabled && <ChevronDown16 className="h-4 w-4 text-hero-dark" />}
				</ButtonOrLink>
			}
		>
			{!!balance && (
				<div className="flex flex-wrap gap-1 justify-end">
					<div className="text-bodySmall font-medium text-hero-darkest/40">Balance</div>{' '}
					<Text variant="bodySmall" weight="medium" color="steel-darker">
						{balance?.formatted} {coinMetadata?.symbol}
					</Text>
				</div>
			)}
		</DescriptionItem>
	);
}
