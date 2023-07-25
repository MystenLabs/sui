// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * 1. WaitForEffectsCert: waits for TransactionEffectsCert and then returns to the client.
 *    This mode is a proxy for transaction finality.
 * 2. WaitForLocalExecution: waits for TransactionEffectsCert and makes sure the node
 *    executed the transaction locally before returning to the client. The local execution
 *    makes sure this node is aware of this transaction when the client fires subsequent queries.
 *    However, if the node fails to execute the transaction locally in a timely manner,
 *    a bool type in the response is set to false to indicate the case.
 */
export type ExecuteTransactionRequestType = 'WaitForEffectsCert' | 'WaitForLocalExecution';

export type SuiTransactionBlockResponseQuery = {
	filter?: TransactionFilter;
	options?: SuiTransactionBlockResponseOptions;
};

export type SuiTransactionBlockResponseOptions = {
	/* Whether to show transaction input data. Default to be false. */
	showInput?: boolean;
	/* Whether to show transaction effects. Default to be false. */
	showEffects?: boolean;
	/* Whether to show transaction events. Default to be false. */
	showEvents?: boolean;
	/* Whether to show object changes. Default to be false. */
	showObjectChanges?: boolean;
	/* Whether to show coin balance changes. Default to be false. */
	showBalanceChanges?: boolean;
};

export type TransactionFilter =
	| { FromOrToAddress: { addr: string } }
	| { Checkpoint: string }
	| { FromAndToAddress: { from: string; to: string } }
	| { TransactionKind: string }
	| {
			MoveFunction: {
				package: string;
				module: string | null;
				function: string | null;
			};
	  }
	| { InputObject: string }
	| { ChangedObject: string }
	| { FromAddress: string }
	| { ToAddress: string };
