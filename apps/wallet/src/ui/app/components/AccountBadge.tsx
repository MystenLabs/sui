// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { BadgeLabel } from './BadgeLabel';
import { AccountType } from '_src/background/keyring/Account';

type AccountBadgeProps = {
	accountType: AccountType;
};

const TYPE_TO_TEXT: Record<AccountType, string | null> = {
	[AccountType.LEDGER]: 'Ledger',
	[AccountType.IMPORTED]: 'Imported',
	[AccountType.QREDO]: 'Qredo',
	[AccountType.DERIVED]: null,
};

export function AccountBadge({ accountType }: AccountBadgeProps) {
	const badgeText = TYPE_TO_TEXT[accountType];

	if (!badgeText) return null;

	return <BadgeLabel label={badgeText} />;
}
