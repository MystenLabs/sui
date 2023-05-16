// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { BadgeLabel } from './BadgeLabel';
import { AccountType } from '_src/background/keyring/Account';

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
        case AccountType.QREDO:
            badgeText = 'Qredo';
            break;
        default:
            throw new Error(`Encountered unknown account type ${accountType}`);
    }

    return badgeText ? <BadgeLabel label={badgeText} /> : null;
}
