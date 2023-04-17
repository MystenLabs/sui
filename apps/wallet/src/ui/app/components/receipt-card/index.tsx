// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowUpRight12 } from '@mysten/icons';
import {
    getExecutionStatusType,
    getTransactionKindName,
    getTotalGasUsed,
    getExecutionStatusError,
    SUI_TYPE_ARG,
    getTransactionKind,
    getTransactionSender,
    getTransactionDigest,
    getGasData,
} from '@mysten/sui.js';
import { useMemo } from 'react';

import { DateCard } from '../../shared/date-card';
import { ReceiptCardBg } from './ReceiptCardBg';
import { SponsoredTxnGasSummary } from './SponsoredTxnGasSummary';
import { StatusIcon } from './StatusIcon';
import { TxnAddressLink } from './TxnAddressLink';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import { StakeTxnCard } from '_components/receipt-card/StakeTxnCard';
import { TxnAddress } from '_components/receipt-card/TxnAddress';
import { TxnAmount } from '_components/receipt-card/TxnAmount';
import { UnStakeTxnCard } from '_components/receipt-card/UnstakeTxnCard';
// import { TxnImage } from '_components/transactions-card/TxnImage';
import { useGetTxnRecipientAddress, useGetTransferAmount } from '_hooks';
import { TxnGasSummary } from '_src/ui/app/components/receipt-card/TxnGasSummary';
import { Text } from '_src/ui/app/shared/text';

import type { SuiTransactionBlockResponse, SuiAddress } from '@mysten/sui.js';

type ReceiptCardProps = {
    txn: SuiTransactionBlockResponse;
    activeAddress: SuiAddress;
};

function ReceiptCard({ txn, activeAddress }: ReceiptCardProps) {
    const { events } = txn;
    const timestamp = txn.timestampMs;
    const executionStatus = getExecutionStatusType(txn);
    const error = useMemo(() => getExecutionStatusError(txn), [txn]);
    const isSuccessful = executionStatus === 'success';
    const transaction = getTransactionKind(txn)!;
    const txnKind = getTransactionKindName(transaction);

    const recipientAddress = useGetTxnRecipientAddress({
        txn,
        address: activeAddress,
    });

    // const objectId = useMemo(() => {
    //     return getTxnEffectsEventID(events!, activeAddress)[0];
    // }, [events, activeAddress]);

    const transferAmount = useGetTransferAmount({
        txn,
        activeAddress,
    });

    const totalSuiAmount = useMemo(() => {
        const amount = transferAmount?.find(
            ({ coinType }) => coinType === SUI_TYPE_ARG
        )?.amount;
        return amount ? Math.abs(amount) : null;
    }, [transferAmount]);

    const { owner } = getGasData(txn)!;
    const transactionSender = getTransactionSender(txn);
    const isSender = activeAddress === transactionSender;
    const isSponsoredTransaction = transactionSender !== owner;
    const gasTotal = Number(getTotalGasUsed(txn));

    const showGasSummary = isSuccessful && isSender && gasTotal;
    const showSponsorInfo = !isSuccessful && isSender && isSponsoredTransaction;
    const stakedTxn = events?.find(
        ({ type }) => type === '0x3::validator::StakingRequestEvent'
    );

    const unstakeTxn = events?.find(
        ({ type }) => type === '0x3::validator::UnstakingRequestEvent'
    );

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

    // const nftObjectLabel = transferAmount?.length ? txnStatusText : 'Call';

    return (
        <div className="block relative w-full">
            <div className="flex mt-2.5 justify-center items-start">
                <StatusIcon status={isSuccessful} />
            </div>
            {timestamp && (
                <div className="my-3 flex justify-center">
                    <DateCard timestamp={Number(timestamp)} size="md" />
                </div>
            )}

            <ReceiptCardBg status={executionStatus}>
                <div className="divide-y divide-solid divide-steel/20 divide-x-0 flex flex-col pt-3.5 first:pt-0">
                    {error && (
                        <div className="py-3.5 first:pt-0 break-words">
                            <Text
                                variant="body"
                                weight="medium"
                                color="issue-dark"
                            >
                                {error}
                            </Text>
                        </div>
                    )}
                    {stakedTxn ? <StakeTxnCard event={stakedTxn} /> : null}
                    {unstakeTxn ? <UnStakeTxnCard event={unstakeTxn} /> : null}

                    <>
                        {/* {objectId && (
                            <TxnImage
                                id={objectId}
                                actionLabel={nftObjectLabel}
                            />
                        )} */}

                        {transferAmount && transferAmount.length > 0
                            ? transferAmount.map(
                                  ({ amount, coinType, receiverAddress }) => {
                                      return (
                                          <div
                                              key={coinType + receiverAddress}
                                              className="divide-y divide-solid divide-steel/20 divide-x-0 flex flex-col pt-3.5 first:pt-0"
                                          >
                                              <TxnAmount
                                                  amount={amount}
                                                  label={txnStatusText}
                                                  coinType={coinType}
                                              />

                                              <TxnAddress
                                                  address={recipientAddress!}
                                                  label={
                                                      isSender ? 'To' : 'From'
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
                                    variant="pBody"
                                    weight="medium"
                                    color="steel-darker"
                                >
                                    Sponsor
                                </Text>
                                <TxnAddressLink address={owner} />
                            </div>
                        )}

                        {txnKind === 'ChangeEpoch' &&
                            !transferAmount?.length && (
                                <TxnAddress
                                    address={recipientAddress!}
                                    label="From"
                                />
                            )}

                        {txnGasSummary}
                    </>

                    <div className="flex gap-1.5 w-full py-3.5">
                        <ExplorerLink
                            type={ExplorerLinkType.transaction}
                            transactionID={getTransactionDigest(txn)}
                            title="View on Sui Explorer"
                            className="text-sui-dark text-pSubtitleSmall font-semibold no-underline uppercase tracking-wider"
                            showIcon={false}
                        >
                            View on Explorer
                        </ExplorerLink>
                        <ArrowUpRight12 className="text-steel text-pSubtitle" />
                    </div>
                </div>
            </ReceiptCardBg>
        </div>
    );
}

export default ReceiptCard;
