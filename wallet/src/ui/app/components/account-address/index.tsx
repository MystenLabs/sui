// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import CopyToClipboard from '_components/copy-to-clipboard';
import { useAppSelector, useMiddleEllipsis } from '_hooks';

import st from './AccountAddress.module.scss';

function AccountAddress() {
    const address = useAppSelector(
        ({ account: { address } }) => address && `0x${address}`
    );
    const shortenAddress = useMiddleEllipsis(address || '');
    return address ? (
        <CopyToClipboard txt={address}>
            <span className={st.address} title={address}>
                {shortenAddress}
            </span>
        </CopyToClipboard>
    ) : null;
}

export default AccountAddress;
