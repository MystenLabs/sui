// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
  ExecuteTransactionRequestType,
  SuiClient,
  SuiTransactionBlockResponse,
  SuiTransactionBlockResponseOptions,
} from '@mysten/sui.js/client';
import type { Keypair, SignatureWithBytes } from '@mysten/sui.js/cryptography';

import { Level, logger } from './logger';
import { Pool } from './pool';
import type { SplitStrategy } from './splitStrategies';
import type { TransactionBlockWithLambda } from './transactions';
import type { TransactionBlock } from '@mysten/sui.js/transactions';
import type { Signature } from './types';

/**
 * A class that orchestrates the execution of transaction blocks using multiple worker pools.
 * The workers are created by splitting a main pool and are used to execute transaction blocks asynchronously without object equivocation.
 * [Note: ] The mainPool is not a worker pool and is not used for transaction block execution. It is used only for splitting.
 * The number of workers is not fixed and can be increased by splitting the main pool if the workload requires it.
 * The ExecutorServiceHandler retries the execution of the transaction block up to a specified number of times in case of errors.
 */
export class ExecutorServiceHandler {
  private _mainPool: Pool; // shared resource between threads
  private _workersQueue: Pool[] = []; // shared resource between threads
  private _accessQueue = Promise.resolve(); // mutex for thread safety of mainPool access
  private readonly _getWorkerTimeoutMs: number;
  private constructor(mainPool: Pool, getWorkerTimeoutMs: number) {
    this._mainPool = mainPool;
    this._getWorkerTimeoutMs = getWorkerTimeoutMs;
  }

  /**
   * Initializes an ExecutorServiceHandler instance.
   * @param keypair - The keypair to use for authentication.
   * @param client - The SuiClient instance to use for communication with the Sui network.
   * @param getWorkerTimeoutMs - The maximum number of milliseconds to listen for an available
   * worker from the worker queue.
   * @returns A new ExecutorServiceHandler instance.
   */
  public static async initialize(
    keypair: Keypair,
    client: SuiClient,
    getWorkerTimeoutMs = 10000,
  ) {
    const pool = await Pool.full({ keypair: keypair, client });
    return new ExecutorServiceHandler(pool, getWorkerTimeoutMs);
  }

  /**
   * Executes the given transaction block using the provided SuiClient and split strategy (if any).
   * Retries the execution up to the specified number of times in case of errors.
   *
   * Note that the execution is asynchronous and the result is returned as a Promise.
   * This means that you can execute multiple transaction blocks in parallel **without**
   * equivocating objects, as long as the splitStrategy permits it.
   * @param txb The transaction block to execute.
   * @param client The SuiClient instance is to use it for execution.
   * @param splitStrategy The SplitStrategy used to determine how a new worker pool will be split
   * from the main pool in case a new worker is needed to execute the transaction.
   * @param options (Optional) The SuiTransactionBlockResponseOptions to use for executing the transaction block.
   * @param requestType (Optional) The ExecuteTransactionRequestType to use for executing the transaction block.
   * @param retries The maximum number of retries in case of errors (default: 3).
   * @param sponsorLambda (Optional) A function that acts upon the transaction block before execution.
   * Useful for sponsoring transactions.
   * @returns A Promise that resolves to the result of the transaction block execution.
   * @throws An error if all retries fail.
   */
  public async execute(
    txb: TransactionBlockWithLambda,
    client: SuiClient,
    splitStrategy?: SplitStrategy,
    options?: SuiTransactionBlockResponseOptions,
    requestType?: ExecuteTransactionRequestType,
    sponsorLambda?: (
      txb: TransactionBlock,
    ) => Promise<[SignatureWithBytes, SignatureWithBytes | Signature]>,
    retries = 3,
  ) {
    let res;
    const flowId = Pool.generateShortGUID();
    do {
      try {
        res = await this.executeFlow(
          flowId,
          txb,
          client,
          splitStrategy,
          options,
          requestType,
          sponsorLambda,
        );
      } catch (e) {
        logger.log(
          Level.error,
          `${flowId} - ESHandler: Error executing transaction block: ${e} - ${
            retries - 1
          } retries left...`,
        );
        continue;
      }
      if (res) {
        logger.log(
          Level.info,
          `${flowId} - ESHandler: Transaction block execution completed - digest: ${JSON.stringify(
            res.digest,
          )}`,
        );
        return res;
      }
      logger.log(
        Level.debug,
        `${flowId} - ESHandler: Could not execute flow: unavailable worker - ${
          retries - 1
        } retries left...`,
      );
    } while (--retries > 0);
    logger.log(
      Level.error,
      `${flowId} - ESHandler: executeFlowError - All retries failed: Could not execute the transaction block`,
    );
    throw new Error(
      `${flowId} - ESHandler: executeFlowError - All retries failed: Could not execute the transaction block`,
    );
  }

  /**
   * Helper function of execute(). Contains the main logic for executing a transaction block,
   * including getting an available worker from the workers array, updating the workerPool status, etc.
   * @param flowId - flowId for logging purposes
   * @param txb The transaction block to execute.
   * @param client The SuiClient to use for executing the transaction block.
   * @param options (Optional) The SuiTransactionBlockResponseOptions to use for executing the transaction block.
   * @param requestType (Optional) The ExecuteTransactionRequestType to use for executing the transaction block.
   * @param splitStrategy (Optional) The SplitStrategy to use for splitting the main pool and getting a new worker pool.
   * @param sponsorLambda (Optional) A function that acts upon the transaction block just before execution.
   * Use it to generate a sender and sponsor signature for the transaction block.
   * @returns A Promise that resolves to the SuiTransactionBlockResponse object returned by executing the transaction block.
   */
  private async executeFlow(
    flowId: string,
    txb: TransactionBlockWithLambda,
    client: SuiClient,
    splitStrategy?: SplitStrategy,
    options?: SuiTransactionBlockResponseOptions,
    requestType?: ExecuteTransactionRequestType,
    sponsorLambda?: (
      txb: TransactionBlock,
    ) => Promise<[SignatureWithBytes, SignatureWithBytes | Signature]>,
  ) {
    let worker: Pool | undefined;
    try {
      worker = await this.getAWorker(flowId);
    } catch (e) {
      worker = undefined;
    }
    const noWorkerAvailable = worker === undefined;
    if (noWorkerAvailable) {
      logger.log(
        Level.debug,
        `${flowId} - ESHandler: Could not find an available worker.`,
      );
      await this.passToAccessQueue(async () => {
        await this.addWorker(flowId, client, splitStrategy);
      });
      return;
    } else if (worker) {
      logger.log(
        Level.debug,
        `${flowId} - ESHandler: Found an available worker: ${worker.id}. Executing transaction block...`,
      );
      let result: SuiTransactionBlockResponse;
      try {
        result = await worker.signAndExecuteTransactionBlock({
          transactionBlockLambda: txb,
          client: client,
          options,
          requestType,
          sponsorLambda,
        });
      } catch (e) {
        logger.log(
          Level.warn,
          `${flowId} - ESHandler: Error executing transaction block: ${e}`,
        );
        await this.passToAccessQueue(async () => {
          if (worker) {
            this._mainPool.merge(worker);
          }
        });
        return;
      }

      if (result.effects && result.effects.status.status === 'failure') {
        logger.log(
          Level.error,
          `${flowId} - ESHandler: Error executing transaction block: result status is "failure"`,
        );
        await this.passToAccessQueue(async () => {
          if (worker) {
            this._mainPool.merge(worker);
          }
        });
        return;
      }
      logger.log(
        Level.debug,
        `${flowId} - ESHandler: Transaction block execution completed! Pushing worker ${worker.id} back to the queue...`,
      );
      // Execution finished, the worker is now available again.
      this._workersQueue.push(worker);
      return result;
    }
  }

  /**
   * Returns an available worker from the worker queue, or undefined if none are available within the timeout period.
   * @returns {Pool | undefined} - An available worker from the worker queue,
   * or undefined if none are available within the timeout period.
   */
  private async getAWorker(flowId: string): Promise<Pool | undefined> {
    logger.log(
      Level.debug,
      `${flowId} - ESHandler: Getting a worker from the queue...`,
    );
    const timeoutMs = this._getWorkerTimeoutMs;
    const startTime = new Date().getTime();

    const tryGetWorker = (): Promise<Pool | undefined> => {
      return new Promise((resolve) => {
        const tryNext = () => {
          const worker = this._workersQueue.pop();
          if (worker) {
            resolve(worker);
          } else if (new Date().getTime() - startTime >= timeoutMs) {
            logger.log(
              Level.debug,
              `${flowId} - ESHandler: Timeout reached - no available worker found`,
            );
            resolve(undefined);
          } else {
            setTimeout(tryNext, 100);
          }
        };

        tryNext();
      });
    };

    return await tryGetWorker();
  }

  /**
   * Adds a new worker pool to the worker queue.
   * @param flowId - flowId for logging purposes
   * @param client - The SuiClient instance to use it for the execution of transactions by the new worker pool.
   * @param splitStrategy - (Optional) The SplitStrategy to use for splitting the main pool and creating the new pool.
   */
  private async addWorker(
    flowId: string,
    client: SuiClient,
    splitStrategy?: SplitStrategy,
  ) {
    logger.log(
      Level.debug,
      `${flowId} - ESHandler: Adding new worker to the queue...`,
    );
    const newPool = await this._mainPool.split(client, splitStrategy);
    logger.log(
      Level.debug,
      `${flowId} - ESHandler: New worker added to the queue: ${
        newPool.id
      } - ${Array.from(newPool.objects.keys())}`,
    );
    this._workersQueue.push(newPool);
  }

  /**
   * Passes the given function to the access queue to ensure thread safety of mainPool.
   * @param fn - The function to pass to the access queue.e.g., this._mainPool.addWorker(...)
   * @private
   */
  private async passToAccessQueue(fn: () => Promise<void>) {
    this._accessQueue = this._accessQueue.then(fn);
    await this._accessQueue;
  }
}
