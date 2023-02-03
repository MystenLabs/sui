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
} from '@mysten/sui.js';
import { useMemo } from 'react';

import { DateCard } from '../../shared/date-card';
import { ReceiptCardBg } from './ReceiptCardBg';
import { StatusIcon } from './StatusIcon';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import { DelegationObjectCard } from '_components/receipt-card/DelegationObjectCard';
import { StakeTxnCard } from '_components/receipt-card/StakeTxnCard';
import { TxnAddress } from '_components/receipt-card/TxnAddress';
import { TxnAmount } from '_components/receipt-card/TxnAmount';
import { TxnGasSummery } from '_components/receipt-card/TxnGasSummery';
import { UnStakeTxnCard } from '_components/receipt-card/UnstakeTxnCard';
import { getTxnEffectsEventID } from '_components/transactions-card';
import { TxnImage } from '_components/transactions-card/TxnImage';
import { getEventsSummary, checkStakingTxn } from '_helpers';
import { useGetTxnRecipientAddress } from '_hooks';
import { Text } from '_src/ui/app/shared/text';

import type { SuiTransactionResponse, SuiAddress } from '@mysten/sui.js';

type ReceiptCardProps = {
    txn: SuiTransactionResponse;
    activeAddress: SuiAddress;
};

function ReceiptCard({ txn, activeAddress }: ReceiptCardProps) {
    const { timestamp_ms, certificate, effects } = txn;
    const executionStatus = getExecutionStatusType(txn);
    const error = useMemo(() => getExecutionStatusError(txn), [txn]);
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

    const gasTotal = getTotalGasUsed(txn);

    const moveCallLabel = useMemo(() => {
        if (txnKind !== 'Call') return null;
        const moveCallLabel = checkStakingTxn(txn);
        return moveCallLabel ? moveCallLabel : 'Call';
    }, [txn, txnKind]);

    const transferAmount = useMemo(() => {
        return eventsSummary.filter(
            ({ receiverAddress }) => receiverAddress === activeAddress
        );
    }, [eventsSummary, activeAddress]);

    const totalSuiAmount = useMemo(() => {
        const amount = eventsSummary.find(
            ({ receiverAddress, coinType }) =>
                receiverAddress === activeAddress && coinType === SUI_TYPE_ARG
        )?.amount;
        return amount ? Math.abs(amount) : null;
    }, [activeAddress, eventsSummary]);

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
                    <>
                        {objectId && (
                            <TxnImage
                                id={objectId}
                                label={isSender ? 'Sent' : 'Received'}
                            />
                        )}

                        {transferAmount.length > 0
                            ? transferAmount.map(
                                  ({ amount, coinType, receiverAddress }) => {
                                      return (
                                          <div
                                              key={coinType + receiverAddress}
                                              className="divide-y divide-solid divide-steel/20 divide-x-0 gap-3.5 flex flex-col"
                                          >
                                              <TxnAmount
                                                  amount={amount}
                                                  label={
                                                      amount > 0
                                                          ? 'Received'
                                                          : 'Sent'
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
                    </>
                )}

                {txnKind === 'ChangeEpoch' && !transferAmount.length ? (
                    <DelegationObjectCard senderAddress={recipientAddress} />
                ) : null}

                {gasTotal && isSender ? (
                    <TxnGasSummery
                        totalGas={gasTotal}
                        transferAmount={totalSuiAmount}
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
                    <ArrowUpRight12 className="text-steel text-p3" />
                </div>
            </ReceiptCardBg>
        </div>
    );
}

export default ReceiptCard;
