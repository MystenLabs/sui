// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TxnAddress } from '_components/receipt-card/TxnAddress';

//TODO: depending on what to show on delegation object card, we may need to add more fields or moved this into receipt card
export function DelegationObjectCard({
    senderAddress,
}: {
    senderAddress: string;
}) {
    return <TxnAddress address={senderAddress} label="From" />;
}
