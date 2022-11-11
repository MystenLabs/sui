// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CoinBalance } from '~/ui/CoinBalance';
import { Heading } from '~/ui/Heading';
import { SenderRecipientAddress } from '~/ui/SenderRecipientAddress';

type Recipient = {
    address: string;
    coin?: {
        amount: number | string;
        symbol?: string;
    };
};

export interface SenderRecipientProps {
    sender: string;
    transferCoin?: boolean;
    recipients?: Recipient[];
}

export function SenderRecipient({
    sender,
    recipients = [],
    transferCoin,
}: SenderRecipientProps) {
    const multipleRecipients = recipients.length > 1;
    const singleTransferCoin = !!(
        !multipleRecipients &&
        transferCoin &&
        recipients.length
    );
    const primaryRecipient = singleTransferCoin && recipients[0];
    const multipleRecipientsList = primaryRecipient
        ? recipients.slice(1)
        : recipients;

    return (
        <div className="flex flex-col justify-start text-sui-grey-100 gap-4">
            <Heading variant="heading4" weight="semibold">
                {singleTransferCoin ? 'Sender & Recipient' : 'Sender'}
            </Heading>
            <div className="flex flex-col gap-[15px] justify-center relative">
                {singleTransferCoin && (
                    <div className="absolute border-2 border-sui-steel overflow-y-hidden h-[calc(55%)] w-4 border-r-transparent border-t-transparent mt-1 ml-1.5 rounded-l border-dotted" />
                )}
                <SenderRecipientAddress isSender address={sender} />
                {primaryRecipient && (
                    <div className="ml-6">
                        <SenderRecipientAddress
                            address={primaryRecipient.address}
                        />
                    </div>
                )}
                {multipleRecipientsList?.length ? (
                    <div className="mt-3.5 flex flex-col gap-2.5">
                        <div className="mb-2.5">
                            <Heading variant="heading4" weight="semibold">
                                {multipleRecipientsList.length > 1
                                    ? 'Recipients'
                                    : 'Recipient'}
                            </Heading>
                        </div>

                        <div className="flex flex-col gap-2">
                            {multipleRecipientsList.map((recipient) => (
                                <div
                                    className="flex flex-col gap-0.5"
                                    key={recipient.address}
                                >
                                    <SenderRecipientAddress
                                        address={recipient?.address}
                                    />
                                    {recipient?.coin && (
                                        <div className="ml-6">
                                            <CoinBalance
                                                amount={recipient.coin.amount}
                                                symbol={recipient.coin?.symbol}
                                            />
                                        </div>
                                    )}
                                </div>
                            ))}
                        </div>
                    </div>
                ) : null}
            </div>
        </div>
    );
}
