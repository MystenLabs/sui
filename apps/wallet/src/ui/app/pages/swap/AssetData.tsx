// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { Heading } from '_app/shared/heading';
import { Text } from '_app/shared/text';
import { ButtonOrLink } from '_app/shared/utils/ButtonOrLink';
import { CoinIcon } from '_components/coin-icon';
import { DescriptionItem } from '_pages/approval-request/transaction-request/DescriptionList';
import { ChevronDown16 } from '@mysten/icons';

export function AssetData({
	tokenBalance,
	coinType,
	symbol,
	to,
	onClick,
	disabled,
}: {
	tokenBalance: string;
	coinType: string;
	symbol: string;
	to?: string;
	onClick?: () => void;
	disabled?: boolean;
}) {
	return (
		<DescriptionItem
			title={
				<div className="flex gap-1 items-center">
					<CoinIcon coinType={coinType} size="sm" />
					<ButtonOrLink
						disabled={disabled}
						onClick={onClick}
						to={to}
						className="flex gap-1 items-center no-underline outline-none border-transparent bg-transparent p-0"
					>
						<Heading variant="heading6" weight="semibold" color="hero-dark">
							{symbol}
						</Heading>
						{!disabled && <ChevronDown16 className="h-4 w-4 text-hero-dark" />}
					</ButtonOrLink>
				</div>
			}
		>
			{!!tokenBalance && (
				<div className="flex gap-1">
					<div className="text-bodySmall font-medium text-hero-darkest/40">Balance</div>{' '}
					<Text variant="bodySmall" weight="medium" color="steel-darker">
						{tokenBalance} {symbol}
					</Text>
				</div>
			)}
		</DescriptionItem>
	);
}
