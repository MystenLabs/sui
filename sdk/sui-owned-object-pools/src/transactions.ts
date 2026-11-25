// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import type { TransactionBlock } from '@mysten/sui.js/transactions';

export type TransactionBlockLambda = (...args: any[]) => TransactionBlock;

/**
 * A class that represents a transaction block with a lambda function.
 *
 * This is a wrapper class that allows you to pass the transaction block
 * creation inside a worker pool. The owned `objectIds` of the objects
 * that are needed to be passed as arguments in the transaction block,
 * are filled automatically by the worker pool.
 *
 * e.g., you have a transaction block that needs to mint an NFT,
 * but you need to pass an `AdminCap` to do so.
 *
 * Using this class you can create the transaction block in the lambda function,
 * and use a placeholder for the `AdminCap` object.
 * You also need to provide to the `lambdaArgs` the type of the object,
 * so the worker pool finds the objectId of the owned object -in this case
 * the `AdminCap`- by looking up its contents and fetching it by the provided type.
 *
 * i.e.,
 * ```
 * lambda = (adminCap: string) => {
 *  const txb = new TransactionBlock();
 *  txb.mintNFT(
 *    txb.object(
 *      adminCap, // This is the placeholder for the AdminCap object.
 *    "NFT"
 *    );
 *  return txb;
 * }
 * ```
 * `lambdaArgs = ["AdminCap"]`
 */
export class TransactionBlockWithLambda {
  public lambda: TransactionBlockLambda;
  public lambdaArgs?: any[] = [];

  /**
   * Initializes a TransactionBlockWithLambda instance.
   * @param lambda The lambda function used to create a transaction block.
   * @param lambdaArgs The lambda function arguments.
   * Use this if you need to pass owned objects as arguments to your transaction
   * block.
   */
  constructor(lambda: TransactionBlockLambda, lambdaArgs: any[] = []) {
    this.lambda = lambda;
    this.lambdaArgs = lambdaArgs;
  }
}
