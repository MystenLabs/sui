// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { DateCard } from '_app/shared/date-card';
import { Text } from '_app/shared/text';
import { useGetTxnRecipientAddress } from '_hooks';
import { useRecognizedPackages } from '_src/ui/app/hooks/useRecognizedPackages';
import { getLabel, useTransactionSummary } from '@mysten/core';
import type { SuiTransactionBlockResponse } from '@mysten/sui/client';
import { Link } from 'react-router-dom';

import { TxnTypeLabel } from './TxnActionLabel';
import { TxnIcon } from './TxnIcon';

export function TransactionCard({
	txn,
	address,
}: {
	txn: SuiTransactionBlockResponse;
	address: string;
}) {
	const executionStatus = txn.effects?.status.status;
	const recognizedPackagesList = useRecognizedPackages();

	const summary = useTransactionSummary({
		transaction: txn,
		currentAddress: address,
		recognizedPackagesList,
	});

	// we only show Sui Transfer amount or the first non-Sui transfer amount

	const recipientAddress = useGetTxnRecipientAddress({ txn, address });

	const isSender = address === txn.transaction?.data.sender;

	const error = txn.effects?.status.error;

	// Transition label - depending on the transaction type and amount
	// Epoch change without amount is delegation object
	// Special case for staking and unstaking move call transaction,
	// For other transaction show Sent or Received

	// TODO: Support programmable tx:
	// Show sui symbol only if transfer transferAmount coinType is SUI_TYPE_ARG, staking or unstaking
	const showSuiSymbol = false;

	const timestamp = txn.timestampMs;

	return (
		<Link
			data-testid="link-to-txn"
			to={`/receipt?${new URLSearchParams({
				txdigest: txn.digest,
			}).toString()}`}
			className="flex items-center w-full flex-col gap-2 py-4 no-underline"
		>
			<div className="flex items-start w-full justify-between gap-3">
				<div className="w-7.5">
					<TxnIcon
						txnFailed={executionStatus !== 'success' || !!error}
						// TODO: Support programmable transactions variable icons here:
						variant={getLabel(txn, address)}
					/>
				</div>
				<div className="flex flex-col w-full gap-1.5">
					{error ? (
						<div className="flex w-full justify-between">
							<div className="flex flex-col w-full gap-1.5">
								<Text color="gray-90" weight="medium">
									Transaction Failed
								</Text>

								<div className="flex break-all">
									<Text variant="pSubtitle" weight="normal" color="issue-dark">
										{error}
									</Text>
								</div>
							</div>
							{/* {transferAmountComponent} */}
						</div>
					) : (
						<>
							<div className="flex w-full justify-between">
								<div className="flex gap-1 align-middle items-baseline">
									<Text color="gray-90" weight="semibold">
										{summary?.label}
									</Text>
									{showSuiSymbol && (
										<Text color="gray-90" weight="normal" variant="subtitleSmall">
											SUI
										</Text>
									)}
								</div>
								{/* {transferAmountComponent} */}
							</div>

							{/* TODO: Support programmable tx: */}
							<TxnTypeLabel address={recipientAddress!} isSender={isSender} isTransfer={false} />
							{/* {objectId && <TxnImage id={objectId} />} */}
						</>
					)}

					{timestamp && <DateCard timestamp={Number(timestamp)} size="sm" />}
				</div>
			</div>
		</Link>
	);
}
