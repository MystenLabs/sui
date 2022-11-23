// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CoinFormat } from '~/hooks/useFormatCoin';
import { CoinBalance } from '~/ui/CoinBalance';
import { Heading } from '~/ui/Heading';
import { SenderRecipientAddress } from '~/ui/SenderRecipientAddress';

type Recipient = {
    address: string;
    coin?: {
        amount: number | string | bigint;
        coinType?: string | null;
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
        <div className="flex flex-col justify-start gap-4">
            <Heading variant="heading4" weight="semibold" color="gray-90">
                {singleTransferCoin ? 'Sender & Recipient' : 'Sender'}
            </Heading>
            <div className="flex flex-col gap-[15px] justify-center relative">
                {singleTransferCoin && (
                    <div className="absolute border-dashed border border-steel overflow-y-hidden h-[calc(57%)] w-4 border-r-transparent border-t-transparent mt-1 ml-1.5 rounded-l" />
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
                            <Heading
                                variant="heading4"
                                weight="semibold"
                                color="gray-90"
                            >
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
                                                coinType={
                                                    recipient.coin?.coinType
                                                }
                                                format={CoinFormat.FULL}
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
