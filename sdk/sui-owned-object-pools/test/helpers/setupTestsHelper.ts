// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import type { SuiObjectRef, SuiObjectResponse } from '@mysten/sui.js/client/';
import { SuiClient } from '@mysten/sui.js/client';
import type { Ed25519Keypair } from '@mysten/sui.js/keypairs/ed25519';
import { TransactionBlock } from '@mysten/sui.js/transactions';

import { isCoin } from '../../src/helpers';
import { getAllCoinsFromClient } from './helpers';
import { getKeyPair } from './helpers';
import type { EnvironmentVariables } from './setupEnvironmentVariables';
import { getEnvironmentVariables } from './setupEnvironmentVariables';

/**
 * A helper class for setting up tests. It provides methods for ensuring that
 * the admin has enough coins and objects to run the tests.
 */
export class SetupTestsHelper {
  public MINIMUM_COIN_BALANCE: number;
  private readonly env: EnvironmentVariables;
  private client: SuiClient;
  private adminKeypair: Ed25519Keypair;

  public readonly objects: SuiObjectResponse[] = [];
  private suiCoins: SuiObjectResponse[] = [];

  constructor() {
    this.env = getEnvironmentVariables('../test/.env');
    this.MINIMUM_COIN_BALANCE = 300000000;
    this.client = new SuiClient({
      url: this.env.SUI_NODE,
    });
    this.adminKeypair = getKeyPair(this.env.ADMIN_SECRET_KEY);
  }

  /**
   * Sets up the admin by ensuring they have enough objects and coins.
   * @param minimumObjectsNeeded The minimum number of objects the admin needs.
   * @param minimumCoinsNeeded The minimum number of coins the admin needs.
   */
  public async setupAdmin(
    minimumObjectsNeeded: number,
    minimumCoinsNeeded: number,
  ) {
    const setup = async () => {
      await this.parseCurrentGasCoinsAndObjects();
      await this.assureAdminHasEnoughObjects(minimumObjectsNeeded);
      await this.assureAdminHasMoreThanEnoughCoins(minimumCoinsNeeded);
    };
    try {
      await setup();
    } catch (e) {
      console.warn('SetupTestsHelper - Setup failed: Retrying admin setup...');
      await setup();
    }
  }

  private async parseCurrentGasCoinsAndObjects() {
    let cursor: string | null | undefined = null;
    let resp;
    do {
      resp = await this.client.getOwnedObjects({
        owner: this.adminKeypair.toSuiAddress(),
        options: {
          showContent: true,
          showType: true,
        },
        cursor,
      });
      resp?.data.forEach((object) => {
        if (isCoin(object?.data?.type ?? '')) {
          this.suiCoins.push(object);
        } else {
          this.objects.push(object);
        }
      });
      cursor = resp?.nextCursor;
    } while (resp?.hasNextPage);
  }

  /*
  Reassure that the admin has enough coins and if not add them to him
   */
  private async assureAdminHasMoreThanEnoughCoins(minimumCoinsNeeded: number) {
    let coinToSplit: SuiObjectResponse | undefined;
    if (this.suiCoins.length >= minimumCoinsNeeded) {
      return;
    }
    for (let i = 0; i < minimumCoinsNeeded - this.suiCoins.length; i++) {
      coinToSplit = this.suiCoins.find((coin) => {
        const content = coin.data?.content;
        if (content && 'fields' in content && 'balance' in content.fields) {
          return (
            Number(content.fields?.balance ?? '0') >
            2 * this.MINIMUM_COIN_BALANCE
          );
        } else {
          return false;
        }
      });
      if (!coinToSplit) {
        throw new Error(
          `No coin with enough balance found. \
            To add new coins to account by splitting \
            you need at least ${2 * this.MINIMUM_COIN_BALANCE}`,
        );
      }
      const coinToSplitId = coinToSplit.data?.objectId;
      if (coinToSplitId) {
        await this.addNewCoinToAccount(coinToSplitId);
      }
    }
  }

  private async assureAdminHasEnoughObjects(numberOfObjectsNeeded: number) {
    while (this.objects.length < numberOfObjectsNeeded) {
      await this.addNewObjectToAccount();
    }
  }

  private async addNewObjectToAccount() {
    const mintAndTransferTxb = new TransactionBlock();
    const hero = mintAndTransferTxb.moveCall({
      arguments: [
        mintAndTransferTxb.object(this.env.NFT_APP_ADMIN_CAP),
        mintAndTransferTxb.pure('zed'),
        mintAndTransferTxb.pure('gold'),
        mintAndTransferTxb.pure(3),
        mintAndTransferTxb.pure('ipfs://example.com/'),
      ],
      target: `${this.env.NFT_APP_PACKAGE_ID}::hero_nft::mint_hero`,
    });
    // Transfer to self
    mintAndTransferTxb.transferObjects(
      [hero],
      mintAndTransferTxb.pure(this.adminKeypair.getPublicKey().toSuiAddress()),
    );
    mintAndTransferTxb.setGasBudget(10000000);
    mintAndTransferTxb.setGasPayment(
      this.suiCoins.map((coin) => this.toSuiObjectRef(coin)),
    );
    await this.client.signAndExecuteTransactionBlock({
      transactionBlock: mintAndTransferTxb,
      requestType: 'WaitForLocalExecution',
      options: {
        showEffects: true,
        showEvents: true,
        showObjectChanges: true,
      },
      signer: this.adminKeypair,
    });
  }

  private async addNewCoinToAccount(cointToSplit: string) {
    const txb = new TransactionBlock();
    const coinToPay = await this.client.getObject({ id: cointToSplit });
    const newcoins1 = txb.splitCoins(txb.gas, [
      txb.pure(this.MINIMUM_COIN_BALANCE),
    ]);
    const newcoins2 = txb.splitCoins(txb.gas, [
      txb.pure(this.MINIMUM_COIN_BALANCE),
    ]);
    txb.transferObjects(
      [newcoins1, newcoins2],
      txb.pure(this.adminKeypair.toSuiAddress()),
    );
    txb.setGasBudget(100000000);
    txb.setGasPayment([this.toSuiObjectRef(coinToPay)]);
    await this.client
      .signAndExecuteTransactionBlock({
        signer: this.adminKeypair,
        transactionBlock: txb,
        requestType: 'WaitForLocalExecution',
        options: {
          showEffects: true,
          showObjectChanges: true,
        },
      })
      .then((txRes) => {
        const status = txRes.effects?.status?.status;
        if (status !== 'success') {
          throw new Error(
            `Failed to split and add new coin to admin account! ${status}`,
          );
        }
      })
      .catch((err) => {
        throw new Error(
          `Failed to split coin <${cointToSplit}> and add new coin to admin account! ${err}`,
        );
      });
  }

  private toSuiObjectRef(coin: SuiObjectResponse): SuiObjectRef {
    const data = coin.data;
    if (!data?.objectId || !data?.digest || !data?.version) {
      throw new Error('Invalid coin - missing data');
    }
    return {
      objectId: data?.objectId,
      digest: data?.digest,
      version: data?.version,
    };
  }

  /// Execute a fault TXB that smashes all coins into 1
  /// Used to reset the coins of the admin account.
  /// Very useful for testing to avoid having to remnant coins with low balance
  public async smashCoins(minNumCoins = 10) {
    const coins = await getAllCoinsFromClient(
      this.client,
      this.adminKeypair.getPublicKey().toSuiAddress(),
    );
    const enoughCoins = coins.size >= minNumCoins;
    const enoughBalancePerCoin = Array.from(coins.values()).every((value) => {
      return parseInt(value.balance) >= 100000000;
    });
    if (enoughCoins && enoughBalancePerCoin) {
      console.log('SetupTestsHelper - No need to smash coins.');
      return;
    }
    try {
      const transactionBlock = new TransactionBlock();
      transactionBlock.moveCall({
        arguments: [
          transactionBlock.object(this.env.NFT_APP_ADMIN_CAP),
          transactionBlock.pure('zed'),
          transactionBlock.pure('gold'),
          transactionBlock.pure(3),
          transactionBlock.pure('ipfs://example.com/'),
        ],
        target: `${this.env.NFT_APP_PACKAGE_ID}::hero_nft::mint_hero`,
      });
      transactionBlock.setGasBudget(100000000);
      const res = await this.client.signAndExecuteTransactionBlock({
        transactionBlock,
        requestType: 'WaitForLocalExecution',
        signer: this.adminKeypair,
      });
      if ((res?.effects?.status?.status ?? '') == 'success') {
        console.log('SetupTestsHelper - Smash coins succeeded!');
      } else {
        console.warn(
          'SetupTestsHelper - Smash coins failed! Could not get status.',
        );
      }
    } catch (e) {
      console.warn('SetupTestsHelper - Smash coins failed!', e);
    }
  }
}
