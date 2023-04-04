// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { AccountType } from '_src/background/keyring/Account';
import { Text } from '_src/ui/app/shared/text';

type AccountBadgeProps = {
    accountType: AccountType;
};

const TYPE_TO_TEXT: Record<AccountType, string | null> = {
    [AccountType.LEDGER]: 'Ledger',
    [AccountType.IMPORTED]: 'Imported',
    [AccountType.DERIVED]: 'Derived',
};

export function AccountBadge({ accountType }: AccountBadgeProps) {
    const badgeText = TYPE_TO_TEXT[accountType];

    if (!badgeText) return null;

    return (
        <div className="bg-gray-40 rounded-2xl border border-solid border-gray-45 py-1 px-1.5">
            <Text variant="captionSmallExtra" color="steel-dark">
                {badgeText}
            </Text>
        </div>
    );
}
