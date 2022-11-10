// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Layout from '_components/menu/content/layout';
import { useNextMenuUrl } from '_components/menu/hooks';
import NetworkSelector from '_components/network-selector';

function Network() {
    const mainMenuUrl = useNextMenuUrl(true, '/');
    return (
        <Layout backUrl={mainMenuUrl} title="Network" isSettings>
            <NetworkSelector />
        </Layout>
    );
}

export default Network;
