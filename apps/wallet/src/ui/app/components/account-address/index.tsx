// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';

import CopyToClipboard from '_components/copy-to-clipboard';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import { useAppSelector, useMiddleEllipsis } from '_hooks';
import { API_ENV } from '_src/shared/api-env';

import st from './AccountAddress.module.scss';

type AccountAddressProps = {
    className?: string;
    showLink?: boolean;
    shorten?: boolean;
    copyable?: boolean;
    mode?: 'normal' | 'faded';
};

function AccountAddress({
    className,
    showLink = true,
    shorten = true,
    copyable,
    mode = 'normal',
}: AccountAddressProps) {
    const network = useAppSelector(({ app }) => app.apiEnv);
    const showExplorerLink = API_ENV.customRPC !== network;

    const address = useAppSelector(({ account: { address } }) => address);
    const shortenAddress = useMiddleEllipsis(address, 10, 6);
    const cpIconMode = mode === 'normal' ? 'normal' : 'highlighted';

    const addressLink = address && (
        <span className={cl(st.address, st[mode])} title={address}>
            {shorten ? shortenAddress : address}
        </span>
    );

    return address ? (
        <span className={cl(st.addressContainer, className)}>
            {copyable ? (
                <CopyToClipboard
                    txt={address}
                    mode={cpIconMode}
                    copySuccessMessage="Address copied"
                >
                    {addressLink}
                </CopyToClipboard>
            ) : (
                addressLink
            )}

            {showLink && showExplorerLink ? (
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
