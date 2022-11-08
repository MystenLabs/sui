// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva } from 'class-variance-authority';

import { ReactComponent as DoneIcon } from '~/assets/SVGIcons/CheckFill.svg';
import { ReactComponent as StartIcon } from '~/assets/SVGIcons/Start.svg';
import { Link} from '~/ui/Link';
import { Heading } from '~/ui/Heading';
import { Text } from '~/ui/Text';

const senderRecipientAddressStyles = cva(
    ['break-all flex flex-row gap-2 w-full items-center'],
    {
        variants: {
            isCoinTransfer: {
                true: 'ml-6',
            },
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
    const isCoinTransfer = !!(isCoin && !isSender);
    return (
        <div className={senderRecipientAddressStyles({ isCoinTransfer })}>
            <div className="w-4 mt-1">
                {isSender ? <StartIcon /> : <DoneIcon />}
            </div>
            <Link variant="mono" to={`/addresses/${encodeURIComponent(address)}`}>
                {address}
            </Link>
        </div>
    );
}

function Amount({ amount, symbol }: { amount: number; symbol?: string }) {
    return (
        <div className="flex flex-row items-end gap-1 text-sui-grey-100 ml-6">
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
