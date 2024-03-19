// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SerializedUIAccount } from '_src/background/accounts/Account';
import { CheckFill16 } from '@mysten/icons';

import { Link } from '../../shared/Link';
import { Text } from '../../shared/text';
import { Tooltip } from '../../shared/tooltip';
import { AccountListItem } from './AccountListItem';

export type RecoverAccountsGroupProps = {
	title: string;
	accounts: SerializedUIAccount[];
	showRecover?: boolean;
	onRecover?: () => void;
	recoverDone?: boolean;
};

export function RecoverAccountsGroup({
	title,
	accounts,
	showRecover,
	onRecover,
	recoverDone,
}: RecoverAccountsGroupProps) {
	return (
		<div className="flex flex-col items-stretch w-full gap-4">
			<div className="flex flex-nowrap items-center gap-1 px-2">
				<Text variant="caption" weight="semibold" color="steel-dark">
					{title}
				</Text>
				<div className="h-px bg-gray-45 flex flex-1 flex-shrink-0" />
				<div>
					{showRecover && !recoverDone ? (
						<Link
							size="bodySmall"
							color="hero"
							weight="semibold"
							text="Recover"
							onClick={onRecover}
						/>
					) : null}
					{recoverDone ? (
						<Tooltip tip="Recovery process done">
							<CheckFill16 className="text-success w-4 h-4" />
						</Tooltip>
					) : null}
				</div>
			</div>
			{accounts.map((anAccount) => (
				<AccountListItem key={anAccount.id} account={anAccount} />
			))}
		</div>
	);
}
