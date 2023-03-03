// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatAddress, type SuiAddress } from '@mysten/sui.js';
import cl from 'classnames';

import CopyToClipboard from '_components/copy-to-clipboard';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import { useAppSelector } from '_hooks';
import { API_ENV } from '_src/shared/api-env';

import st from './AccountAddress.module.scss';

type AccountAddressProps = {
    className?: string;
    showLink?: boolean;
    shorten?: boolean;
    copyable?: boolean;
    mode?: 'normal' | 'faded';
    address?: SuiAddress;
};

function AccountAddress({
    className,
    showLink = true,
    shorten = true,
    copyable,
    mode = 'normal',
    address,
}: AccountAddressProps) {
    const network = useAppSelector(({ app }) => app.apiEnv);
    const showExplorerLink = API_ENV.customRPC !== network;
    const activeAddress = useAppSelector(({ account }) => account.address);
    const addressToShow = address || activeAddress;
    const cpIconMode = mode === 'normal' ? 'normal' : 'highlighted';

    const addressLink = addressToShow && (
        <span className={cl(st.address, st[mode])} title={addressToShow}>
            {shorten ? formatAddress(addressToShow) : addressToShow}
        </span>
    );

    return addressToShow ? (
        <span className={cl(st.addressContainer, className)}>
            {copyable ? (
                <CopyToClipboard
                    txt={addressToShow}
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
