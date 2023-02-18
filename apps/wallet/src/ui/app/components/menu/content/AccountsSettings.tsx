// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';

import { Account } from './Account';
import { MenuLayout } from './MenuLayout';
import { useNextMenuUrl } from '_components/menu/hooks';
import { FEATURES } from '_src/shared/experimentation/features';
import { useAccounts } from '_src/ui/app/hooks/useAccounts';
import { useDeriveNextAccountMutation } from '_src/ui/app/hooks/useDeriveNextAccountMutation';
import { Button } from '_src/ui/app/shared/ButtonUI';

export function AccountsSettings() {
    const backUrl = useNextMenuUrl(true, '/');
    const accounts = useAccounts();
    const isMultiAccountsEnabled = useFeature(
        FEATURES.WALLET_MULTI_ACCOUNTS
    ).on;
    const createAccountMutation = useDeriveNextAccountMutation();
    return (
        <MenuLayout title="Accounts" back={backUrl}>
            <div className="flex flex-col gap-3">
                {accounts.map(({ address }) => (
                    <Account address={address} key={address} />
                ))}
                {isMultiAccountsEnabled ? (
                    <Button
                        variant="outline"
                        size="tall"
                        text="Create New Account"
                        loading={createAccountMutation.isLoading}
                        onClick={() => createAccountMutation.mutate()}
                    />
                ) : null}
            </div>
        </MenuLayout>
    );
}
