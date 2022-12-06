// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ReactComponent as Recipient } from './icons/checkfill.svg';
import { ReactComponent as Sender } from './icons/sender.svg';

import { Link } from '~/ui/Link';

export type SenderRecipientAddressProps = {
    isSender?: boolean;
    address: string;
};

export function SenderRecipientAddress({
    isSender,
    address,
}: SenderRecipientAddressProps) {
    return (
        <div className="flex items-center gap-2 break-all">
            <div className="mt-1 w-4">
                {isSender ? <Sender /> : <Recipient />}
            </div>

            <Link
                variant="mono"
                size="md"
                to={`/address/${encodeURIComponent(address)}`}
            >
                {address}
            </Link>
        </div>
    );
}
