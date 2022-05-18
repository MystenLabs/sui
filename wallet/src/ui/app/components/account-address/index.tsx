// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useMemo, useState } from 'react';

import BsIcon from '_components/bs-icon';
import { useAppSelector } from '_hooks';

import st from './AccountAddress.module.scss';

const COPY_CHECKMARK_MILLIS = 600;

// TODO: make copy to clipboard reusable
function AccountAddress() {
    const address = useAppSelector(
        ({ account: { address } }) => address && `0x${address}`
    );
    const shortenAddress = useMemo(() => {
        if (!address) {
            return '';
        }
        return `${address.substring(0, 7)}...${address.substring(
            address.length - 7
        )}`;
    }, [address]);
    const [copied, setCopied] = useState(false);
    const copyToClipboard = useCallback(async () => {
        if (!address) {
            return;
        }
        await navigator.clipboard.writeText(address);
        setCopied(true);
    }, [address]);
    useEffect(() => {
        let timeout: number;
        if (copied) {
            timeout = window.setTimeout(
                () => setCopied(false),
                COPY_CHECKMARK_MILLIS
            );
        }
        return () => {
            if (timeout) {
                clearTimeout(timeout);
            }
        };
    }, [copied]);
    return address ? (
        <span className={st.address} title={address} onClick={copyToClipboard}>
            {shortenAddress}
            <BsIcon
                className={st['copy-icon']}
                icon={`clipboard${copied ? '-check' : ''}`}
            />
        </span>
    ) : null;
}

export default AccountAddress;
