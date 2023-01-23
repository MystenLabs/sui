// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useMemo } from 'react';

import Icon, { SuiIcons } from '_components/icon';

interface TxnItemIconProps {
    txnFailed?: boolean;
    isSender: boolean;
}

const icons = {
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
};

export function TxnIcon({ txnFailed, isSender }: TxnItemIconProps) {
    const variant = useMemo(() => {
        return isSender ? 'Send' : 'Receive';
    }, [isSender]);

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
