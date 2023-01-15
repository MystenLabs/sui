// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getExecutionStatusType,
    getTransactionKindName,
    getMoveCallTransaction,
} from '@mysten/sui.js';
import { cva, type VariantProps } from 'class-variance-authority';
import cl from 'classnames';
import { useMemo } from 'react';

import { Text } from '_app/shared/text';
import Icon, { SuiIcons } from '_components/icon';
import { useAppSelector } from '_hooks';

import type {
    TransactionKindName,
    SuiTransactionResponse,
} from '@mysten/sui.js';

type TxnIconType = TransactionKindName | 'Failure' | 'Minted' | 'Swapped';

type txnIconName = 'Minted' | 'Swapped' | 'Send' | 'Receive';

const iconsStyles = cva([], {
    variants: {
        direction: {
            send: 'rotate-135',
            receive: '-rotate-45',
            none: 'rotate-0',
        },

        failed: {
            true: 'text-issue-dark',
            false: 'text-gradient-blue-start',
        },
    },

    defaultVariants: {
        direction: 'none',
        failed: false,
    },
});

export interface TxnIconProps extends VariantProps<typeof iconsStyles> {
    iconName: TxnIconType;
    variant?: 'Minted' | 'Swapped' | 'Failure';
}

function TxIcon({ iconName, variant, ...styleProps }: TxnIconProps) {
    const icons = {
        Minted: SuiIcons.Buy,
        Failure: SuiIcons.Info,
        Swapped: SuiIcons.Swap,
    };

    const icon = variant ? icons[variant] : SuiIcons.ArrowLeft;
    return <Icon icon={icon} className={iconsStyles(styleProps)} />;
}

interface TxnItemIconProps {
    txnKindName: TransactionKindName | 'Minted';
    txnFailed?: boolean;
    isSender: boolean;
}

export function TxnItemIcon({
    txnKindName,
    txnFailed,
    isSender,
}: TxnItemIconProps) {
    const variant = useMemo(() => {
        if (txnKindName === 'Minted') return 'Minted';
        return isSender ? 'Send' : 'Receive';
    }, [txnFailed, txnKindName]);

    const direction = isSender ? 'send' : 'receive';

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
                'w-7.5 h-7.5 flex justify-center items-center rounded-2lg ',
            ])}
        >
            {icons[variant]}
        </div>
    );
}

export function TxnItem({ txn }: { txn: SuiTransactionResponse }) {
    const address = useAppSelector(({ account: { address } }) => address);
    const { certificate } = txn;
    const executionStatus = getExecutionStatusType(txn) as 'Success' | 'Failed';
    const txnKind = getTransactionKindName(certificate.data.transactions[0]);
    const moveCallTxn = getMoveCallTransaction(
        certificate.data.transactions[0]
    );
    const isSender = certificate.data.sender === address;
    const txnIconName =
        txnKind === 'Call' && moveCallTxn?.function === 'mint'
            ? 'Minted'
            : txnKind;

    const txnName =
        txnKind === 'Call' && moveCallTxn?.function === 'mint'
            ? 'Minted'
            : txnKind;

    return (
        <div className="flex items-center w-full flex-col gap-2">
            <div className="flex items-center w-full justify-between gap-3">
                <TxnItemIcon
                    txnKindName={txnIconName}
                    txnFailed={executionStatus === 'Failed'}
                    isSender={isSender}
                />
                <div className="flex flex-col w-full">
                    <Text color="gray-90" weight="semibold">
                        {isSender ? 'Sent' : 'Received'} {txnKind}
                    </Text>
                </div>
            </div>
        </div>
    );
}
