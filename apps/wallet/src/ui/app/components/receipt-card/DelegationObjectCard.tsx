// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TxnAddress } from '_components/receipt-card/TxnAddress';

export function DelegationObjectCard({
    senderAddress,
}: {
    senderAddress: string;
}) {
    return <TxnAddress address={senderAddress} label="From" />;
}
