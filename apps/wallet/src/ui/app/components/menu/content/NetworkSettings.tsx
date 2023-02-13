// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useNextMenuUrl } from '_components/menu/hooks';
import NetworkSelector from '_components/network-selector';
import PageTitle from '_src/ui/app/shared/PageTitle';

export function NetworkSettings() {
    const mainMenuUrl = useNextMenuUrl(true, '/');
    return (
        <>
            <PageTitle title="Network" back={mainMenuUrl} />
            <NetworkSelector />
        </>
    );
}
