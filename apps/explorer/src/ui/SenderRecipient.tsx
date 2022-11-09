// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Amount } from '~/ui/Amount';
import { Heading } from '~/ui/Heading';
import { SenderRecipientAddress } from '~/ui/SenderRecipientAddress';

type Recipient = {
    address: string;
    coin?: {
        amount: number;
        coinSymbol?: string;
    };
};

export interface SenderRecipientProps {
    sender: string;
    transferCoin?: boolean;
    recipients?: Recipient[];
}

export function SenderRecipient({
    sender,
    recipients,
    transferCoin,
}: SenderRecipientProps) {
    const multipleRecipients = !!(recipients?.length && recipients?.length > 1);
    const singleTransferCoin = !!(
        !multipleRecipients &&
        transferCoin &&
        recipients?.length
    );
    const primaryRecipient = singleTransferCoin && recipients?.[0];
    const multipleRecipientsList = primaryRecipient
        ? recipients?.filter((_, i) => i !== 0)
        : recipients;

    return (
        <div className="flex flex-col justify-start h-full text-sui-grey-100 gap-4">
            <Heading as="h4" variant="heading4" weight="semibold">
                Sender {singleTransferCoin && '& Recipient'}
            </Heading>
            <div className="flex flex-col gap-[15px] justify-center relative">
                {singleTransferCoin && (
                    <div className="absolute border-2 border-[#a0b6c3] overflow-y-hidden h-[calc(55%)] w-4 border-r-[transparent] border-t-[transparent] mt-1 ml-1.5 rounded-l border-dotted" />
                )}
                <SenderRecipientAddress isSender address={sender} />
                {primaryRecipient && (
                    <SenderRecipientAddress
                        address={primaryRecipient.address}
                        isCoin
                    />
                )}
                {multipleRecipientsList?.length ? (
                    <div className="mt-2 flex flex-col gap-2">
                        <div className=" mt-[5px] mb-2.5">
                            <Heading
                                as="h4"
                                variant="heading4"
                                weight="semibold"
                            >
                                Recipient
                                {multipleRecipientsList.length > 1 && 's'}
                            </Heading>
                        </div>

                        <div className="flex flex-col gap-6">
                            {multipleRecipientsList.map((recipient) => (
                                <div
                                    className="flex flex-col gap-2.5"
                                    key={recipient.address}
                                >
                                    <SenderRecipientAddress
                                        address={recipient?.address}
                                    />
                                    {recipient?.coin && (
                                        <Amount
                                            amount={recipient.coin.amount}
                                            coinSymbol={
                                                recipient.coin?.coinSymbol
                                            }
                                        />
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
