// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { useMutation } from '@tanstack/react-query';
import { toast } from 'react-hot-toast';

import { Account } from './Account';
import { MenuLayout } from './MenuLayout';
import { useNextMenuUrl } from '_components/menu/hooks';
import { FEATURES } from '_src/shared/experimentation/features';
import { useAccounts } from '_src/ui/app/hooks/useAccounts';
import { useBackgroundClient } from '_src/ui/app/hooks/useBackgroundClient';
import { Button } from '_src/ui/app/shared/ButtonUI';

export function AccountsSettings() {
    const backUrl = useNextMenuUrl(true, '/');
    const importPrivateKeyUrl = useNextMenuUrl(true, '/import-private-key');
    const accounts = useAccounts();
    const isMultiAccountsEnabled = useFeature(
        FEATURES.WALLET_MULTI_ACCOUNTS
    ).on;
    const backgroundClient = useBackgroundClient();
    const createAccountMutation = useMutation({
        mutationFn: async () => {
            await backgroundClient.deriveNextAccount();
            return null;
        },
        onSuccess: () => {
            toast.success('New account created');
        },
        onError: (e) => {
            toast.error((e as Error).message || 'Failed to create new account');
        },
    });
    return (
        <MenuLayout title="Accounts" back={backUrl}>
            <div className="flex flex-col gap-3">
                {accounts.map(({ address }) => (
                    <Account address={address} key={address} />
                ))}
                {isMultiAccountsEnabled ? (
                    <>
                        <Button
                            variant="outline"
                            size="tall"
                            text="Create New Account"
                            loading={createAccountMutation.isLoading}
                            onClick={() => createAccountMutation.mutate()}
                        />
                        <Button
                            variant="outline"
                            size="tall"
                            text="Import Private Key"
                            to={importPrivateKeyUrl}
                        />
                    </>
                ) : null}
            </div>
        </MenuLayout>
    );
}
