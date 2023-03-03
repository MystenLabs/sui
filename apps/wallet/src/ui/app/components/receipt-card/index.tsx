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
    getTransactionKinds,
    getTransactionSender,
    getTransactionDigest,
    getGasData,
} from '@mysten/sui.js';
import { useMemo } from 'react';

import { DateCard } from '../../shared/date-card';
import { TxnAddressLink } from '../TxnAddressLink';
import { ReceiptCardBg } from './ReceiptCardBg';
import { SponsoredTxnGasSummary } from './SponsoredTxnGasSummary';
import { StatusIcon } from './StatusIcon';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import { StakeTxnCard } from '_components/receipt-card/StakeTxnCard';
import { TxnAddress } from '_components/receipt-card/TxnAddress';
import { TxnAmount } from '_components/receipt-card/TxnAmount';
import { UnStakeTxnCard } from '_components/receipt-card/UnstakeTxnCard';
import { getTxnEffectsEventID } from '_components/transactions-card';
import { TxnImage } from '_components/transactions-card/TxnImage';
import { checkStakingTxn } from '_helpers';
import { useGetTxnRecipientAddress, useGetTransferAmount } from '_hooks';
import { TxnGasSummary } from '_src/ui/app/components/receipt-card/TxnGasSummary';
import { Text } from '_src/ui/app/shared/text';

import type { SuiTransactionResponse, SuiAddress } from '@mysten/sui.js';

type ReceiptCardProps = {
    txn: SuiTransactionResponse;
    activeAddress: SuiAddress;
};

function ReceiptCard({ txn, activeAddress }: ReceiptCardProps) {
    const { effects, events } = txn;
    const timestamp = txn.timestampMs;
    const executionStatus = getExecutionStatusType(txn);
    const error = useMemo(() => getExecutionStatusError(txn), [txn]);
    const isSuccessful = executionStatus === 'success';
    const [transaction] = getTransactionKinds(txn)!;
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
            : getTxnEffectsEventID(effects!, events!, activeAddress)[0];
    }, [transaction, effects, events, activeAddress]);

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
            ({ coinType }) => coinType === SUI_TYPE_ARG
        )?.amount;
        return amount ? Math.abs(amount) : null;
    }, [transferAmount]);

    const isStakeTxn =
        moveCallLabel === 'Staked' || moveCallLabel === 'Unstaked';

    const { owner } = getGasData(txn)!;
    const transactionSender = getTransactionSender(txn);
    const isSender = activeAddress === transactionSender;
    const isSponsoredTransaction = transactionSender !== owner;
    const gasTotal = getTotalGasUsed(txn);

    const showGasSummary = isSuccessful && isSender && gasTotal;
    const showSponsorInfo = !isSuccessful && isSender && isSponsoredTransaction;

    let txnGasSummary: JSX.Element | undefined;
    if (showGasSummary && isSponsoredTransaction) {
        txnGasSummary = (
            <SponsoredTxnGasSummary sponsor={owner} totalGas={gasTotal} />
        );
    } else if (showGasSummary) {
        txnGasSummary = (
            <TxnGasSummary
                totalGas={gasTotal}
                transferAmount={totalSuiAmount}
            />
        );
    }

    let txnStatusText = '';
    if (isSender && isSuccessful) {
        txnStatusText = 'Sent';
    } else if (isSender && !isSuccessful) {
        txnStatusText = 'Failed to Send';
    } else {
        txnStatusText = 'Received';
    }

    const nftObjectLabel = transferAmount?.length ? txnStatusText : 'Call';

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
                <div className="divide-y divide-solid divide-steel/20 divide-x-0 flex flex-col pt-3.5 first:pt-0">
                    {error && (
                        <div className="py-3.5 first:pt-0">
                            <Text
                                variant="body"
                                weight="medium"
                                color="issue-dark"
                            >
                                {error}
                            </Text>
                        </div>
                    )}

                    {isStakeTxn ? (
                        moveCallLabel === 'Staked' ? (
                            <StakeTxnCard
                                txnEffects={effects!}
                                events={events!}
                            />
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
                                      ({
                                          amount,
                                          coinType,
                                          receiverAddress,
                                      }) => {
                                          return (
                                              <div
                                                  key={
                                                      coinType + receiverAddress
                                                  }
                                                  className="divide-y divide-solid divide-steel/20 divide-x-0 flex flex-col pt-3.5 first:pt-0"
                                              >
                                                  <TxnAmount
                                                      amount={amount}
                                                      label={txnStatusText}
                                                      coinType={coinType}
                                                  />

                                                  <TxnAddress
                                                      address={
                                                          recipientAddress!
                                                      }
                                                      label={
                                                          isSender
                                                              ? 'To'
                                                              : 'From'
                                                      }
                                                  />
                                              </div>
                                          );
                                      }
                                  )
                                : null}

                            {showSponsorInfo && (
                                <div className="flex justify-between items-center py-3.5">
                                    <Text
                                        variant="p1"
                                        weight="medium"
                                        color="steel-darker"
                                    >
                                        Sponsor
                                    </Text>
                                    <TxnAddressLink address={owner} />
                                </div>
                            )}

                            {txnKind === 'ChangeEpoch' &&
                                !transferAmount.length && (
                                    <TxnAddress
                                        address={recipientAddress!}
                                        label="From"
                                    />
                                )}

                            {txnGasSummary}
                        </>
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
                </div>
            </ReceiptCardBg>
        </div>
    );
}

export default ReceiptCard;
