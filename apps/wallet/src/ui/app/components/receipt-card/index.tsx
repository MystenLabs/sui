// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowUpRight12 } from '@mysten/icons';
import {
    getExecutionStatusType,
    getTransferObjectTransaction,
    getTransactionKindName,
    getTotalGasUsed,
    getExecutionStatusError,
    SUI_TYPE_ARG,
    getTransactions,
    getTransactionSender,
    getTransactionDigest,
} from '@mysten/sui.js';
import { useMemo } from 'react';

import { DateCard } from '../../shared/date-card';
import { ReceiptCardBg } from './ReceiptCardBg';
import { StatusIcon } from './StatusIcon';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import { StakeTxnCard } from '_components/receipt-card/StakeTxnCard';
import { TxnAddress } from '_components/receipt-card/TxnAddress';
import { TxnAmount } from '_components/receipt-card/TxnAmount';
import { TxnGasSummery } from '_components/receipt-card/TxnGasSummery';
import { UnStakeTxnCard } from '_components/receipt-card/UnstakeTxnCard';
import { getTxnEffectsEventID } from '_components/transactions-card';
import { TxnImage } from '_components/transactions-card/TxnImage';
import { checkStakingTxn } from '_helpers';
import { useGetTxnRecipientAddress, useGetTransferAmount } from '_hooks';
import { Text } from '_src/ui/app/shared/text';

import type { SuiTransactionResponse, SuiAddress } from '@mysten/sui.js';

type ReceiptCardProps = {
    txn: SuiTransactionResponse;
    activeAddress: SuiAddress;
};

function ReceiptCard({ txn, activeAddress }: ReceiptCardProps) {
    const { effects } = txn;
    const timestamp = txn.timestamp_ms || txn.timestampMs;
    const executionStatus = getExecutionStatusType(txn);
    const error = useMemo(() => getExecutionStatusError(txn), [txn]);
    const isSuccessful = executionStatus === 'success';
    const [transaction] = getTransactions(txn);
    const txnKind = getTransactionKindName(transaction);

    const recipientAddress = useGetTxnRecipientAddress({
        txn,
        address: activeAddress,
    });

    const objectId = useMemo(() => {
        const transferId =
            getTransferObjectTransaction(transaction)?.objectRef?.objectId;

        return transferId
            ? transferId
            : getTxnEffectsEventID(effects, activeAddress)[0];
    }, [activeAddress, transaction, effects]);

    const gasTotal = getTotalGasUsed(txn);

    const moveCallLabel = useMemo(() => {
        if (txnKind !== 'Call') return null;
        const moveCallLabel = checkStakingTxn(txn);
        return moveCallLabel ? moveCallLabel : 'Call';
    }, [txn, txnKind]);

    const transferAmount = useGetTransferAmount({
        txn,
        activeAddress,
    });

    const totalSuiAmount = useMemo(() => {
        const amount = transferAmount.find(
            ({ receiverAddress, coinType }) =>
                receiverAddress === activeAddress && coinType === SUI_TYPE_ARG
        )?.amount;
        return amount ? Math.abs(amount) : null;
    }, [activeAddress, transferAmount]);

    const isSender = activeAddress === getTransactionSender(txn);
    const isStakeTxn =
        moveCallLabel === 'Staked' || moveCallLabel === 'Unstaked';

    const nftObjectLabel = transferAmount?.length
        ? isSender
            ? 'Sent'
            : 'Received'
        : 'Call';

    return (
        <div className="block relative w-full">
            <div className="flex mt-2.5 justify-center items-start">
                <StatusIcon status={isSuccessful} />
            </div>
            {timestamp && (
                <div className="my-3 flex justify-center">
                    <DateCard timestamp={timestamp} size="md" />
                </div>
            )}

            <ReceiptCardBg status={isSuccessful}>
                {error && (
                    <Text variant="body" weight="medium" color="steel-darker">
                        {error}
                    </Text>
                )}

                {isStakeTxn ? (
                    moveCallLabel === 'Staked' ? (
                        <StakeTxnCard txnEffects={effects} />
                    ) : (
                        <UnStakeTxnCard
                            txn={txn}
                            activeAddress={activeAddress}
                            amount={totalSuiAmount || 0}
                        />
                    )
                ) : (
                    <div className="divide-y divide-solid divide-steel/20 divide-x-0 flex flex-col">
                        {objectId && (
                            <div className="py-3.5 first:pt-0 flex gap-2 flex-col">
                                <Text
                                    variant="body"
                                    weight="medium"
                                    color="steel-darker"
                                >
                                    {nftObjectLabel}
                                </Text>
                                <TxnImage id={objectId} />
                            </div>
                        )}

                        {transferAmount.length > 0
                            ? transferAmount.map(
                                  ({ amount, coinType, receiverAddress }) => {
                                      return (
                                          <div
                                              key={coinType + receiverAddress}
                                              className="divide-y divide-solid divide-steel/20 divide-x-0 flex flex-col pt-3.5 first:pt-0"
                                          >
                                              <TxnAmount
                                                  amount={amount}
                                                  label={
                                                      isSender
                                                          ? 'Sent'
                                                          : 'Received'
                                                  }
                                                  coinType={coinType}
                                              />

                                              <TxnAddress
                                                  address={recipientAddress}
                                                  label={
                                                      amount > 0 ? 'From' : 'To'
                                                  }
                                              />
                                          </div>
                                      );
                                  }
                              )
                            : null}

                        {txnKind === 'ChangeEpoch' &&
                            !transferAmount.length && (
                                <TxnAddress
                                    address={recipientAddress}
                                    label="From"
                                />
                            )}

                        {gasTotal && isSender ? (
                            <TxnGasSummery
                                totalGas={gasTotal}
                                transferAmount={totalSuiAmount}
                            />
                        ) : null}
                    </div>
                )}

                <div className="flex gap-1.5 w-full py-3.5">
                    <ExplorerLink
                        type={ExplorerLinkType.transaction}
                        transactionID={getTransactionDigest(txn)}
                        title="View on Sui Explorer"
                        className="text-sui-dark text-p4 font-semibold no-underline uppercase tracking-wider"
                        showIcon={false}
                    >
                        View on Explorer
                    </ExplorerLink>
                    <ArrowUpRight12 className="text-steel text-p3" />
                </div>
            </ReceiptCardBg>
        </div>
    );
}

export default ReceiptCard;