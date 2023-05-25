// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TransactionArgument, TransactionBlock } from '@mysten/sui.js';
import { ObjectArgument, objArg } from '../utils';

/** The Transfer Policy module. */
export const TRANSFER_POLICY_MODULE = '0x2::transfer_policy';

/** The Transer Policy Rules package address */
// TODO: Figure out how we serve this for both testnet & mainnet (different package)
export const TRANSFER_POLICY_RULES_PACKAGE_ADDRESS =
  'bd8fc1947cf119350184107a3087e2dc27efefa0dd82e25a1f699069fe81a585';

/**
 * Call the `transfer_policy::new` function to create a new transfer policy.
 * Returns `transferPolicyCap`
 */
export function createTransferPolicy(
  tx: TransactionBlock,
  itemType: string,
  publisher: ObjectArgument,
): TransactionArgument {
  let [transferPolicy, transferPolicyCap] = tx.moveCall({
    target: `${TRANSFER_POLICY_MODULE}::new`,
    typeArguments: [itemType],
    arguments: [objArg(tx, publisher)],
  });

  tx.moveCall({
    target: `0x2::transfer::public_share_object`,
    typeArguments: [itemType],
    arguments: [transferPolicy],
  });

  return transferPolicyCap;
}

/**
 * Call the `transfer_policy::withdraw` function to withdraw profits from a transfer policy.
 */
export function withdrawFromPolicy(
  tx: TransactionBlock,
  itemType: string,
  policy: ObjectArgument,
  policyCap: ObjectArgument,
  amount: string | bigint | null,
): TransactionArgument {
  let amountArg =
    amount !== null
      ? tx.pure(amount, 'Option<u64>')
      : tx.pure({ None: true }, 'Option<u64>');

  let [profits] = tx.moveCall({
    target: `${TRANSFER_POLICY_MODULE}::withdraw`,
    typeArguments: [itemType],
    arguments: [objArg(tx, policy), objArg(tx, policyCap), amountArg],
  });

  return profits;
}

/**
 * Call the `transfer_policy::confirm_request` function to unblock the
 * transaction.
 */
export function confirmRequest(
  tx: TransactionBlock,
  itemType: string,
  policy: ObjectArgument,
  request: TransactionArgument,
): void {
  tx.moveCall({
    target: `${TRANSFER_POLICY_MODULE}::confirm_request`,
    typeArguments: [itemType],
    arguments: [objArg(tx, policy), request],
  });
}

/**
 * Calls the `transfer_policy::remove_rule` function to remove a Rule from the transfer policy's ruleset.
 */
export function removeTransferPolicyRule(
  tx: TransactionBlock,
  itemType: string,
  ruleType: string,
  configType: string,
  policy: ObjectArgument,
  policyCap: TransactionArgument,
): void {
  tx.moveCall({
    target: `${TRANSFER_POLICY_MODULE}::remove_rule`,
    typeArguments: [itemType, ruleType, configType],
    arguments: [objArg(tx, policy), policyCap],
  });
}

/**
 * Calculates the amount to be paid for the royalty rule to be resolved,
 * splits the coin to pass the exact amount,
 * then calls the `royalty_rule::pay` function to resolve the royalty rule.
 */
export function resolveRoyaltyRule(
  tx: TransactionBlock,
  itemType: string,
  price: string,
  policyId: string,
  transferRequest: TransactionArgument,
) {
  const policyObj = objArg(tx, policyId);
  // calculates the amount
  const [amount] = tx.moveCall({
    target: `${TRANSFER_POLICY_RULES_PACKAGE_ADDRESS}::royalty_rule::fee_amount`,
    typeArguments: [itemType],
    arguments: [policyObj, objArg(tx, price)],
  });

  // splits the coin.
  const feeCoin = tx.splitCoins(tx.gas, [amount]);

  // pays the policy
  tx.moveCall({
    target: `${TRANSFER_POLICY_RULES_PACKAGE_ADDRESS}::royalty_rule::pay`,
    typeArguments: [itemType],
    arguments: [policyObj, transferRequest, feeCoin],
  });
}
