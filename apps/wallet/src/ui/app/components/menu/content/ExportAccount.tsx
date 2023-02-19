// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMutation } from '@tanstack/react-query';
import { Navigate, useParams } from 'react-router-dom';

import { HideShowDisplayBox } from '../../HideShowDisplayBox';
import Alert from '../../alert';
import { useNextMenuUrl } from '../hooks';
import { MenuLayout } from './MenuLayout';
import { PasswordInputDialog } from './PasswordInputDialog';
import { useBackgroundClient } from '_src/ui/app/hooks/useBackgroundClient';

export function ExportAccount() {
    const accountUrl = useNextMenuUrl(true, `/accounts`);
    const { account } = useParams();
    const backgroundClient = useBackgroundClient();
    const exportMutation = useMutation({
        mutationKey: ['export-account', account],
        mutationFn: async (password: string) => {
            if (!account) {
                return null;
            }
            return await backgroundClient.exportAccount(password, account);
        },
    });
    if (!account) {
        return <Navigate to={accountUrl} replace />;
    }
    if (exportMutation.data) {
        return (
            <MenuLayout title="Your Private Key" back={accountUrl}>
                <div className="flex flex-col flex-nowrap items-stretch gap-3">
                    <Alert mode="warning">
                        Do not share your private key! It provides full control
                        of your account.
                    </Alert>
                    <HideShowDisplayBox
                        value={exportMutation.data.privateKey}
                        copiedMessage="Private key copied"
                    />
                </div>
            </MenuLayout>
        );
    }
    return (
        <PasswordInputDialog
            title="Export Private Key"
            onPasswordVerified={async (password) => {
                await exportMutation.mutateAsync(password);
            }}
        />
    );
}
