// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import AccountAddress from '_components/account-address';
import { useNextMenuUrl } from '_components/menu/hooks';
import PageTitle from '_src/ui/app/shared/PageTitle';
import { Text } from '_src/ui/app/shared/text';

export function AccountSettings() {
    const backUrl = useNextMenuUrl(true, '/');
    return (
        <>
            <PageTitle title="Account" back={backUrl} />
            <div className="flex flex-col gap-3 px-2.5 mt-1.5">
                <Text color="gray-90" weight="medium">
                    Address
                </Text>
                <AccountAddress
                    mode="faded"
                    shorten={true}
                    showLink={false}
                    copyable
                />
            </div>
        </>
    );
}
