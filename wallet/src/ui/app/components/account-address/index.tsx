// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';

import CopyToClipboard from '_components/copy-to-clipboard';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import { useAppSelector, useMiddleEllipsis } from '_hooks';

import st from './AccountAddress.module.scss';

type AccountAddressProps = {
    className?: string;
    showLink?: boolean;
};

function AccountAddress({ className, showLink = true }: AccountAddressProps) {
    const address = useAppSelector(
        ({ account: { address } }) => address && `0x${address}`
    );
    const shortenAddress = useMiddleEllipsis(address || '', 20);
    return address ? (
        <span className={cl(st.addressContainer, className)}>
            <CopyToClipboard txt={address}>
                <span className={st.address} title={address}>
                    {shortenAddress}
                </span>
            </CopyToClipboard>
            {showLink ? (
                <ExplorerLink
                    type={ExplorerLinkType.address}
                    useActiveAddress={true}
                    title="View account on Sui Explorer"
                    className={st.explorerLink}
                />
            ) : null}
        </span>
    ) : null;
}

export default AccountAddress;
