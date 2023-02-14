// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { MenuLayout } from './MenuLayout';
import AccountAddress from '_components/account-address';
import { useNextMenuUrl } from '_components/menu/hooks';
import { Text } from '_src/ui/app/shared/text';

export function AccountSettings() {
    const backUrl = useNextMenuUrl(true, '/');
    return (
        <MenuLayout title="Account" back={backUrl}>
            <div className="flex flex-col gap-3">
                <Text color="gray-90" weight="medium" variant="p1">
                    Address
                </Text>
                <AccountAddress
                    mode="faded"
                    shorten={true}
                    showLink={false}
                    copyable
                />
            </div>
        </MenuLayout>
    );
}
