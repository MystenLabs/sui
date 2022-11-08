// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva } from 'class-variance-authority';

//TODO: Switch to using ButtonOrLink component after support for sui explorer links
import Longtext from '~/components/longtext/Longtext';
import { Heading } from '~/ui/Heading';
import { Text } from '~/ui/Text';

const isCoinTransfer = cva(
    ['flex flex-col ml-6 gap-[15px] justify-center  relative'],
    {
        variants: {
            senderRecipient: {
                true: 'before:content-[url()] before:border-2 before:border-[#a0b6c3] before:overflow-y-hidden before:absolute before:h-[calc(55%)] before:w-[15px] before:border-r-[transparent] before:border-t-[transparent] before:mt-1 before:ml-[-16px] before:rounded-l before:border-dotted',
            },
        },
    }
);

const senderRecipientAddressStyles = cva(
    [
        'break-all before:ml-[-23px] before:absolute relative before:top-[50%] before:translate-y-[-50%]',
    ],
    {
        variants: {
            isSender: {
                true: 'before:content-[url(~/assets/SVGIcons/Start.svg)] before:mt-[2px]',
                false: 'before:content-[url(~/assets/SVGIcons/CheckFill.svg)] before:mt-[3px]',
            },
            isCoinTransfer: {
                true: 'ml-[20px]',
            },
        },

        defaultVariants: {
            isSender: false,
        },
    }
);

function SenderRecipientAddress({
    isSender,
    address,
    isCoin,
}: {
    isSender?: boolean;
    address: string;
    isCoin?: boolean;
}) {
    const isCoinTransfer = isCoin && !isSender ? true : false;
    return (
        <div
            className={senderRecipientAddressStyles({
                isSender,
                isCoinTransfer,
            })}
        >
            <Longtext
                text={address}
                category="addresses"
                isLink
                alttext={address}
            />
        </div>
    );
}

function Amount({ amount, symbol }: { amount: number; symbol?: string }) {
    return (
        <div className="flex flex-row items-end gap-1 text-sui-grey-100">
            <Heading as="h4" variant="heading4">
                {amount}
            </Heading>
            {symbol && (
                <div className="text-sui-grey-80">
                    <Text variant="bodySmall">{symbol}</Text>
                </div>
            )}
        </div>
    );
}

type Recipient = {
    address: string;
    amount?: {
        value: number;
        unit?: string;
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
    transferCoin = true,
}: SenderRecipientProps) {
    const multipleRecipients =
        recipients?.length && recipients?.length > 1 ? true : false;
    const senderRecipient = !multipleRecipients && transferCoin ? true : false;
    const primaryRecipient = senderRecipient && recipients?.[0];
    const multipleRecipientsList = primaryRecipient
        ? recipients?.filter((_, i) => i !== 0)
        : recipients;

    return (
        <div className="flex flex-col justify-start h-full text-sui-grey-100 gap-4">
            <Heading as="h4" variant="heading4" weight="semibold">
                Sender {senderRecipient && '& Recipient'}
            </Heading>
            <div className={isCoinTransfer({ senderRecipient })}>
                <SenderRecipientAddress isSender address={sender} />
                {primaryRecipient && (
                    <SenderRecipientAddress
                        address={primaryRecipient.address}
                        isCoin
                    />
                )}
                {multipleRecipientsList?.length ? (
                    <div className="mt-2 flex flex-col gap-2">
                        <div className="ml-[-24px] mt-[5px] mb-[10px]">
                            <Heading
                                as="h4"
                                variant="heading4"
                                weight="semibold"
                            >
                                Recipient
                                {multipleRecipientsList.length > 1 && 's'}
                            </Heading>
                        </div>
                        {multipleRecipientsList?.map((recipient, index) => (
                            <div
                                className="flex flex-col gap-2.5 mb-2"
                                key={index}
                            >
                                <SenderRecipientAddress
                                    address={recipient?.address}
                                />
                                {recipient?.amount && (
                                    <Amount
                                        amount={recipient.amount?.value}
                                        symbol={recipient.amount?.unit}
                                    />
                                )}
                            </div>
                        ))}
                    </div>
                ) : null}
            </div>
        </div>
    );
}
