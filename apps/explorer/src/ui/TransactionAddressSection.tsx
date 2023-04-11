// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CoinFormat } from '@mysten/core';
import { CheckFill16 } from '@mysten/icons';

import { CoinBalance } from './CoinBalance';
import { Heading } from './Heading';
import { TransactionAddress } from './TransactionAddress';

type SponsorTransactionAddressProps = {
    sponsor: string;
};

export function SponsorTransactionAddress({
    sponsor,
}: SponsorTransactionAddressProps) {
    return (
        <TransactionAddressSection title="Sponsor">
            <TransactionAddress
                icon={<CheckFill16 className="text-hero" />}
                address={sponsor}
            />
        </TransactionAddressSection>
    );
}

type SenderTransactionAddressProps = {
    sender: string;
};

export function SenderTransactionAddress({
    sender,
}: SenderTransactionAddressProps) {
    return (
        <TransactionAddressSection title="Sender">
            <TransactionAddress
                icon={<CheckFill16 className="text-steel" />}
                address={sender}
            />
        </TransactionAddressSection>
    );
}

type RecipientTransactionAddressesProps = {
    recipients: {
        amount?: bigint | number | null;
        coinType?: string | null;
        address: string;
    }[];
};

export function RecipientTransactionAddresses({
    recipients,
}: RecipientTransactionAddressesProps) {
    return (
        <TransactionAddressSection
            title={recipients.length > 1 ? 'Recipients' : 'Recipient'}
        >
            <div className="flex flex-col gap-4">
                {recipients.map(({ address, amount, coinType }, i) => (
                    <div
                        className="flex flex-col gap-0.5"
                        key={`${address}-${i}`}
                    >
                        <TransactionAddress
                            icon={<CheckFill16 className="text-success" />}
                            address={address}
                        />
                        {amount ? (
                            <div className="ml-6">
                                <CoinBalance
                                    amount={amount}
                                    coinType={coinType}
                                    format={CoinFormat.FULL}
                                />
                            </div>
                        ) : null}
                    </div>
                ))}
            </div>
        </TransactionAddressSection>
    );
}

type TransactionAddressSectionProps = {
    title: string;
    children: React.ReactNode;
};

function TransactionAddressSection({
    title,
    children,
}: TransactionAddressSectionProps) {
    return (
        <section>
            <Heading variant="heading4/semibold" color="gray-90">
                {title}
            </Heading>
            <div className="mt-4">{children}</div>
        </section>
    );
}
