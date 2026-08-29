// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { isCoin } from './helpers';
import type { PoolObject } from './types';

/**
 * A strategy containing the rules that determine how the split of the pool will be done.
 *
 * - pred: A predicate function used to split the pool's objects and coins into a new pool.
 * This predicate is called for each object, and depending on what it returns,
 * the object will be moved to the new pool, stay in the current pool, or the split will be terminated.
 * The predicate should return:
 * 1. `true`, if the object will be moved to the new Pool
 * 2. `false`, if the object stays in `this` Pool
 * 3. `null`, if the split should be terminated and the new Pool should be returned immediately,
 * with the remaining unchecked objects being kept to the initial pool.
 *
 * [WARNING] If you want to implement a custom strategy, make sure that the predicate
 * will select at least one coin to be moved to the new pool, otherwise the transaction block
 * will not be able to determine the gas payment and will fail.
 *
 * - succeeded: A function that is called after the split is done to check if the split utilized the strategy as supposed to.
 */
export type SplitStrategy = {
  pred: (obj: PoolObject | undefined) => boolean | null;

  /**
   * Call this function after the split is done to check if the split utilized the strategy as supposed to.
   * Used in order to decide if it should be retried by loading more objects for the strategy to iterate over.
   * @returns A boolean indicating if the split succeeded or not.
   */
  succeeded: () => boolean;
};

/**
 * The DefaultSplitStrategy is used when no other strategy is provided.
 * It moves to the new pool enough gas (SUI) coins so that the sum of their
 * balances is greater or equal a specific threshold.
 */
export class DefaultSplitStrategy implements SplitStrategy {
  static readonly defaultMinimumBalance = 400000000;
  private readonly minimumBalance;
  private balanceSoFar = 0;

  constructor(minimumBalance = DefaultSplitStrategy.defaultMinimumBalance) {
    this.minimumBalance = minimumBalance;
  }

  public pred(obj: PoolObject | undefined) {
    if (!obj) throw new Error('No object found!.');
    if (this.balanceSoFar >= this.minimumBalance) {
      return null;
    }
    if (isCoin(obj.type)) {
      this.balanceSoFar += obj.balance ?? 0;
      return true;
    } else {
      return false;
    }
  }

  public succeeded() {
    return this.balanceSoFar >= this.minimumBalance;
  }
}

/**
 * The IncludeAdminCapStrategy is used when the pool needs to contain an AdminCap object.
 * It moves to the new pool enough gas coins to fulfill the needed balance,
 * and one AdminCap object of the specified package.
 */
export class IncludeAdminCapStrategy implements SplitStrategy {
  private readonly adminCapIdentifier: string;
  private readonly packageId: string;

  private readonly minimumBalance;
  private balanceSoFar = 0;
  private adminCapIncluded = false;

  /**
   * Creates a new instance of the Pool class.
   * @param packageId - The ID of the package containing the AdminCap.
   * @param minimumBalance - The minimum balance of the pool
   * @param adminCapIdentifier - A name used to identify the AdminCap object.
   * (pool balance = sum of its' gas coin balances).
   */
  constructor(
    packageId: string,
    minimumBalance = DefaultSplitStrategy.defaultMinimumBalance,
    adminCapIdentifier = 'AdminCap',
  ) {
    this.packageId = packageId;
    this.minimumBalance = minimumBalance;
    this.adminCapIdentifier = adminCapIdentifier;
  }
  public pred(obj: PoolObject | undefined) {
    if (!obj) throw new Error('No object found!.');
    const terminateWhen =
      this.balanceSoFar >= this.minimumBalance && this.adminCapIncluded;
    if (terminateWhen) {
      return null;
    }
    if (
      !this.adminCapIncluded &&
      obj.type.includes(this.adminCapIdentifier) &&
      obj.type.includes(this.packageId)
    ) {
      this.adminCapIncluded = true;
      return true;
    }
    if (!(this.balanceSoFar >= this.minimumBalance) && isCoin(obj.type)) {
      this.balanceSoFar += obj.balance ?? 0;
      return true;
    } else {
      return false;
    }
  }
  public succeeded() {
    return this.balanceSoFar >= this.minimumBalance && this.adminCapIncluded;
  }
}

/**
 * Similar to IncludeAdminCapStrategy but without containing gas coin objects
 * since this will be used for sponsored transactions.
 */
export class SponsoredAdminCapStrategy implements SplitStrategy {
  private readonly adminCapIdentifier: string;
  private readonly packageId: string;

  private adminCapIncluded = false;

  /**
   * Creates a new instance of the Pool class.
   * @param packageId - The ID of the package containing the AdminCap.
   * @param adminCapIdentifier - A name used to identify the AdminCap object.
   */
  constructor(packageId: string, adminCapIdentifier = 'AdminCap') {
    this.packageId = packageId;
    this.adminCapIdentifier = adminCapIdentifier;
  }
  public pred(obj: PoolObject | undefined) {
    if (!obj) throw new Error('No object found!.');
    if (this.adminCapIncluded) {
      return null;
    }
    if (
      obj.type.includes(this.adminCapIdentifier) &&
      obj.type.includes(this.packageId)
    ) {
      this.adminCapIncluded = true;
      return true;
    }
    return false;
  }
  public succeeded() {
    return this.adminCapIncluded;
  }
}
