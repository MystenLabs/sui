// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getExecutionStatusType,
    getTransferObjectTransaction,
    getTransactionKindName,
    getTotalGasUsed,
    SUI_TYPE_ARG,
} from '@mysten/sui.js';
import { useMemo } from 'react';

import { DateCard } from '../../shared/date-card';
import { ReceiptCardBg } from './ReceiptCardBg';
import { StatusIcon } from './StatusIcon';
import { checkStakingTxn } from './checkStakingTxn';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import Icon, { SuiIcons } from '_components/icon';
import { StakeTxnCard } from '_components/receipt-card/StakeTxnCard';
import { TxnAddress } from '_components/receipt-card/TxnAddress';
import { TxnAmount } from '_components/receipt-card/TxnAmount';
import { TxnGasSummery } from '_components/receipt-card/TxnGasSummery';
import { UnStakeTxnCard } from '_components/receipt-card/UnstakeTxnCard';
import { getTxnEffectsEventID } from '_components/transactions-card/Transaction';
import { TxnImage } from '_components/transactions-card/TxnImage';
import { getEventsSummary, getAmount } from '_helpers';
import { useGetTxnRecipientAddress } from '_hooks';

import type { SuiTransactionResponse, SuiAddress } from '@mysten/sui.js';

type ReceiptCardProps = {
    txn: SuiTransactionResponse;
    activeAddress: SuiAddress;
};

function ReceiptCard({ txn, activeAddress }: ReceiptCardProps) {
    const { timestamp_ms, certificate, effects } = txn;
    const executionStatus = getExecutionStatusType(txn);
    const isSuccessful = executionStatus === 'success';
    const txnKind = getTransactionKindName(certificate.data.transactions[0]);
    const { coins: eventsSummary } = getEventsSummary(effects, activeAddress);
    const recipientAddress = useGetTxnRecipientAddress({
        txn,
        address: activeAddress,
    });

    const objectId = useMemo(() => {
        const transferId = getTransferObjectTransaction(
            certificate.data.transactions[0]
        )?.objectRef?.objectId;

        return transferId
            ? transferId
            : getTxnEffectsEventID(effects, activeAddress)[0];
    }, [activeAddress, certificate.data.transactions, effects]);

    const amountByRecipient = getAmount(
        certificate.data.transactions[0],
        effects
    );

    const transferAmount = useMemo(() => {
        const amount = amountByRecipient && amountByRecipient?.[0];

        const amountTransfers = eventsSummary.find(
            ({ receiverAddress }) => receiverAddress === activeAddress
        );

        return {
            amount: Math.abs(amount?.amount || amountTransfers?.amount || 0),
            coinType:
                amount?.coinType || amountTransfers?.coinType || SUI_TYPE_ARG,
        };
    }, [activeAddress, amountByRecipient, eventsSummary]);

    const gasTotal = getTotalGasUsed(txn);

    const moveCallLabel = useMemo(() => {
        if (txnKind !== 'Call') return null;
        const moveCallLabel = checkStakingTxn(txn);
        return moveCallLabel ? moveCallLabel : 'Call';
    }, [txn, txnKind]);

    const isSender = activeAddress === certificate.data.sender;
    const isStakeTxn =
        moveCallLabel === 'Staked' || moveCallLabel === 'Unstaked';

    return (
        <div className="block relative w-full">
            <div className="flex mt-2.5 justify-center items-start">
                <StatusIcon status={isSuccessful} />
            </div>
            {timestamp_ms && (
                <div className="my-3 flex justify-center">
                    <DateCard timestamp={timestamp_ms} size="md" />
                </div>
            )}

            <ReceiptCardBg status={isSuccessful}>
                {isStakeTxn ? (
                    moveCallLabel === 'Staked' ? (
                        <StakeTxnCard
                            amount={transferAmount.amount}
                            txnEffects={effects}
                        />
                    ) : (
                        <UnStakeTxnCard
                            txn={txn}
                            activeAddress={activeAddress}
                            amount={transferAmount.amount}
                        />
                    )
                ) : (
                    <>
                        {objectId && (
                            <TxnImage
                                id={objectId}
                                label={isSender ? 'Sent' : 'Received'}
                            />
                        )}

                        {transferAmount.amount > 0 ? (
                            <div className="w-full">
                                <TxnAmount
                                    amount={transferAmount.amount}
                                    label={isSender ? 'Sent' : 'Received'}
                                    coinType={transferAmount.coinType}
                                />
                            </div>
                        ) : null}

                        {recipientAddress && (
                            <TxnAddress
                                address={recipientAddress}
                                label={isSender ? 'To' : 'From'}
                            />
                        )}
                    </>
                )}

                {gasTotal && isSender ? (
                    <TxnGasSummery
                        totalGas={gasTotal}
                        transferAmount={
                            transferAmount.amount > 0 &&
                            moveCallLabel !== 'Unstaked'
                                ? transferAmount.amount
                                : null
                        }
                    />
                ) : null}

                <div className="flex gap-1.5 pt-3.75 w-full">
                    <ExplorerLink
                        type={ExplorerLinkType.transaction}
                        transactionID={certificate.transactionDigest}
                        title="View on Sui Explorer"
                        className="text-sui-dark text-p4 font-semibold no-underline uppercase tracking-wider"
                        showIcon={false}
                    >
                        View on Explorer
                    </ExplorerLink>
                    <Icon
                        icon={SuiIcons.ArrowLeft}
                        className="text-steel text-p3 rotate-135"
                    />
                </div>
            </ReceiptCardBg>
        </div>
    );
}

export default ReceiptCard;
