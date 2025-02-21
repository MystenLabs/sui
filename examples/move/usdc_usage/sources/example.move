// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module usdc_usage::example;

use sui::coin::Coin;
use sui::sui::SUI;
use usdc::usdc::USDC;

public struct Sword has key, store {
  id: UID,
  strength: u64
}

public fun buy_sword_with_usdc(
  coin: Coin<USDC>,
  tx_context: &mut TxContext
): Sword {
  let sword = create_sword(coin.value(), tx_context);

  transfer::public_transfer(coin, @0x0); // Essentially burning the coin, would send to actual person in production

  sword
}

public fun buy_sword_with_sui(
  coin: Coin<SUI>,
  tx_context: &mut TxContext
): Sword {
  let sword = create_sword(coin.value(), tx_context);

  transfer::public_transfer(coin, @0x0); // Essentially burning the coin, would send to actual person in production

  sword
}

public fun buy_sword_with_arbitrary_coin<CoinType>(
  coin: Coin<CoinType>,
  tx_context: &mut TxContext
): Sword {
  let sword = create_sword(coin.value(), tx_context);

  transfer::public_transfer(coin, @0x0); // Essentially burning the coin, would send to actual person in production

  sword
}

fun create_sword(strength: u64, tx_context: &mut TxContext): Sword {
  let id = object::new(tx_context);
  Sword { id, strength }
}
