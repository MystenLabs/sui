// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva } from 'class-variance-authority';

import { ReactComponent as Recipient } from './icons/checkfill.svg';
import { ReactComponent as Sender } from './icons/sender.svg';

import { Link } from '~/ui/Link';

const senderRecipientAddressStyles = cva(
    ['break-all flex flex-row gap-2 items-center'],
    {
        variants: {
            isCoinTransfer: {
                true: 'ml-6',
            },
        },
    }
);

export type SenderRecipientAddressProps = {
    isSender?: boolean;
    address: string;
    isCoin?: boolean;
};

export function SenderRecipientAddress({
    isSender,
    address,
    isCoin,
}: SenderRecipientAddressProps) {
    const isCoinTransfer = !!(isCoin && !isSender);
    return (
        <div className={senderRecipientAddressStyles({ isCoinTransfer })}>
            <div className="w-4 mt-1">
                {isSender ? <Sender /> : <Recipient />}
            </div>
            <Link
                variant="mono"
                to={`/addresses/${encodeURIComponent(address)}`}
            >
                {address}
            </Link>
        </div>
    );
}
