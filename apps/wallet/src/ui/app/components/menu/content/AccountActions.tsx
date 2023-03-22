// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiAddress } from '@mysten/sui.js';

import { useNextMenuUrl } from '../hooks';
import { AccountType } from '_src/background/keyring/Account';
import { Link } from '_src/ui/app/shared/Link';
import { Text } from '_src/ui/app/shared/text';

export type AccountActionsProps = {
    accountAddress: SuiAddress;
    accountType: AccountType;
};

export function AccountActions({
    accountAddress,
    accountType,
}: AccountActionsProps) {
    const exportAccountUrl = useNextMenuUrl(true, `/export/${accountAddress}`);
    const canExportPrivateKey =
        accountType === AccountType.DERIVED ||
        accountType === AccountType.IMPORTED;

    return (
        <div className="flex flex-row flex-nowrap items-center flex-1">
            {canExportPrivateKey ? (
                <div>
                    <Link
                        text="Export Private Key"
                        to={exportAccountUrl}
                        color="heroDark"
                        weight="medium"
                    />
                </div>
            ) : (
                <Text variant="bodySmall" weight="medium" color="steel">
                    No actions available
                </Text>
            )}
        </div>
    );
}
