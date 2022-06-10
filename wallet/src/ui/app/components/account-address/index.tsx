// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import CopyToClipboard from '_components/copy-to-clipboard';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import { useAppSelector, useMiddleEllipsis } from '_hooks';

import st from './AccountAddress.module.scss';

function AccountAddress() {
    const address = useAppSelector(
        ({ account: { address } }) => address && `0x${address}`
    );
    const shortenAddress = useMiddleEllipsis(address || '');
    return address ? (
        <span className={st['address-container']}>
            <CopyToClipboard txt={address}>
                <span className={st.address} title={address}>
                    {shortenAddress}
                </span>
            </CopyToClipboard>
            <ExplorerLink
                type={ExplorerLinkType.address}
                useActiveAddress={true}
                title="View account on Sui Explorer"
                className={st.explorerLink}
            />
        </span>
    ) : null;
}

export default AccountAddress;
