// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Account } from './Account';
import { MenuLayout } from './MenuLayout';
import { useNextMenuUrl } from '_components/menu/hooks';
import { useAccounts } from '_src/ui/app/hooks/useAccounts';

export function AccountsSettings() {
    const backUrl = useNextMenuUrl(true, '/');
    const accounts = useAccounts();
    return (
        <MenuLayout title="Accounts" back={backUrl}>
            <div className="flex flex-col gap-3">
                {accounts.map(({ address }) => (
                    <Account address={address} key={address} />
                ))}
            </div>
        </MenuLayout>
    );
}
