// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module metal::random_nft {

  use sui::url::{Self, Url};
  use std::string::{String, utf8};
  use sui::package;
  use sui::display;

  use sui::random::{Random, new_generator};

  const EInvalidParams: u64 = 0;

  const GOLD: u8 = 1;
  const SILVER: u8 = 2;
  const BRONZE: u8 = 3;
  const BRONZE_URL: vector<u8> = b"https://mystenlabs-504371932.imgix.net/nft/bronze.png";
  const SILVER_URL: vector<u8> = b"https://mystenlabs-504371932.imgix.net/nft/silver.png";
  const GOLD_URL: vector<u8> = b"https://mystenlabs-504371932.imgix.net/nft/gold.png";

  public struct AirDropNFT has key, store {
    id: UID,
  }

  public struct MetalNFT has key, store {
    id: UID,
    name: String,
    description: String,
    image_url: Url,
    metal: u8,
  }

  public struct MintingCapability has key {
    id: UID,
  }

  public struct RANDOM_NFT has drop {}

  #[allow(unused_function)]
  fun init(otw: RANDOM_NFT, ctx: &mut TxContext) {

    let publisher = package::claim(otw, ctx);

    let keys = vector[
      utf8(b"metal"),
      utf8(b"name"),
      utf8(b"image_url"),
      utf8(b"description"),
    ];

    let values = vector[
      utf8(b"{metal}"),
      utf8(b"{name}"),
      utf8(b"{image_url}"),
      utf8(b"{description}"),
    ];

    let mut display = display::new_with_fields<MetalNFT>(
      &publisher, keys, values, ctx
    );

    display::update_version(&mut display);

    transfer::public_transfer(display, ctx.sender());
    
    transfer::transfer(
        MintingCapability { id: object::new(ctx) },
        ctx.sender(),
    );

    transfer::public_transfer(AirDropNFT { id: object::new(ctx) }, ctx.sender());

    transfer::public_transfer(publisher, ctx.sender())
  }

  public fun mint(_cap: &MintingCapability, n: u16, ctx: &mut TxContext): vector<AirDropNFT> {
        let mut result = vector[];
        let mut i = 0;
        while (i < n) {
            result.push_back(AirDropNFT { id: object::new(ctx) });
            i = i + 1;
        };
        result
    }

  /// Reveal the metal of the airdrop NFT and convert it to a metal NFT.
  /// This function uses arithmetic_is_less_than to determine the metal of the NFT in a way that consumes the same
  /// amount of gas regardless of the value of the random number.
  entry fun reveal(nft: AirDropNFT, r: &Random, ctx: &mut TxContext) {
    destroy_airdrop_nft(nft);

    let mut generator = new_generator(r, ctx);
    let v = generator.generate_u8_in_range(1, 100);

    let is_gold = arithmetic_is_less_than(v, 11, 100); // probability of 10%
    let is_silver = arithmetic_is_less_than(v, 41, 100) * (1 - is_gold); // probability of 30%
    let is_bronze = (1 - is_gold) * (1 - is_silver); // probability of 60%
    let metal = is_gold * GOLD + is_silver * SILVER + is_bronze * BRONZE;
    let mut metal_url = BRONZE_URL;
    let mut metal_name = b"Bronze".to_string();
    let mut metal_description = b"Common metal".to_string();
    if (is_gold > 0) {
      metal_url = GOLD_URL;
      metal_name = b"Gold".to_string();
      metal_description = b"Rare metal".to_string();
    };
    if (is_silver > 0) {
      metal_url = SILVER_URL;
      metal_name = b"Silver".to_string();
      metal_description = b"Uncommon metal".to_string();
    };

    transfer::public_transfer(
        MetalNFT { id: object::new(ctx), image_url: url::new_unsafe_from_bytes({metal_url}), name: {metal_name}, description: {metal_description}, metal, },
        ctx.sender()
    );
  }

  // Implements "is v < w? where v <= v_max" using integer arithmetic. Returns 1 if true, 0 otherwise.
  // Safe in case w and v_max are independent of the randomenss (e.g., fixed).
  // Does not check if v <= v_max.
  fun arithmetic_is_less_than(v: u8, w: u8, v_max: u8): u8 {
      assert!(v_max >= w && w > 0, EInvalidParams);
      let v_max_over_w = v_max / w;
      let v_over_w = v / w; // 0 if v < w, [1, v_max_over_w] if above
      (v_max_over_w - v_over_w) / v_max_over_w
    }

  fun destroy_airdrop_nft(nft: AirDropNFT) {
    let AirDropNFT { id } = nft;
    object::delete(id)
  }
}
