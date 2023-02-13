// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useNextMenuUrl } from '../hooks';
import AutoLockTimerSelector from './AutoLockTimerSelector';
import PageTitle from '_src/ui/app/shared/PageTitle';
import { Text } from '_src/ui/app/shared/text';

export function AutoLockSettings() {
    const backUrl = useNextMenuUrl(true, '/');
    return (
        <>
            <PageTitle title="Auto Lock" back={backUrl} />
            <div className="flex flex-col gap-3.75 mt-4 px-2.5">
                <Text color="gray-90" weight="medium">
                    Set the idle time in minutes before Sui Wallet locks itself.
                </Text>
                <AutoLockTimerSelector />
            </div>
        </>
    );
}
