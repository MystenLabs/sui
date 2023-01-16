// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useMemo } from 'react';

import Icon, { SuiIcons } from '_components/icon';

import type { TransactionKindName } from '@mysten/sui.js';

interface TxnItemIconProps {
    txnKindName: TransactionKindName | 'Minted';
    txnFailed?: boolean;
    isSender: boolean;
}

export function TxnIcon({
    txnKindName,
    txnFailed,
    isSender,
}: TxnItemIconProps) {
    const variant = useMemo(() => {
        if (txnKindName === 'Minted') return 'Minted';
        return isSender ? 'Send' : 'Receive';
    }, [isSender, txnKindName]);

    const icons = {
        Minted: (
            <Icon icon={SuiIcons.Buy} className="text-gradient-blue-start" />
        ),
        Send: (
            <Icon
                icon={SuiIcons.ArrowLeft}
                className="text-gradient-blue-start rotate-135"
            />
        ),
        Receive: (
            <Icon
                icon={SuiIcons.ArrowLeft}
                className="text-gradient-blue-start -rotate-45"
            />
        ),

        Swapped: (
            <Icon icon={SuiIcons.Swap} className="text-gradient-blue-start" />
        ),
    };

    return (
        <div
            className={cl([
                txnFailed ? 'bg-issue-light' : 'bg-gray-45',
                'w-7.5 h-7.5 flex justify-center items-center rounded-2lg',
            ])}
        >
            {txnFailed ? (
                <Icon
                    icon={SuiIcons.Info}
                    className="text-issue-dark text-body"
                />
            ) : (
                icons[variant]
            )}
        </div>
    );
}
