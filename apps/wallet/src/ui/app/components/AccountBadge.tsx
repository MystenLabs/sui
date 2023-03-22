// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { AccountType } from '_src/background/keyring/Account';
import { Text } from '_src/ui/app/shared/text';

type AccountBadgeProps = {
    accountType: AccountType;
};

export function AccountBadge({ accountType }: AccountBadgeProps) {
    let badgeText: string | null = null;
    switch (accountType) {
        case AccountType.LEDGER:
            badgeText = 'Ledger';
            break;
        case AccountType.IMPORTED:
            badgeText = 'Imported';
            break;
        case AccountType.DERIVED:
            badgeText = null;
            break;
        default:
            throw new Error(`Encountered unknown account type ${accountType}`);
    }

    return badgeText ? (
        <div className="bg-gray-40 rounded-2xl border border-solid border-gray-45 py-1 px-1.5">
            <Text variant="captionSmallExtra" color="steel-dark">
                {badgeText}
            </Text>
        </div>
    ) : null;
}
