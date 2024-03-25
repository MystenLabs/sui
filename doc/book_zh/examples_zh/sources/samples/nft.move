// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module examples::devnet_nft {
    use sui::url::{Self, Url};
    use std::string;
    use sui::object::{Self, ID, UID};
    use sui::event;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    /// 一个允许任何人铸造NFT的示例
    /// An example NFT that can be minted by anybody
    struct DevNetNFT has key, store {
        id: UID,
        /// Name for the token 代币（NFT）名
        name: string::String,
        /// Description of the token 代币（NFT）描述
        description: string::String,
        /// URL for the token 代币（NFT）链接
        url: Url,
        // TODO: allow custom attributes 
    }

    // ===== Events ===== 事件

    struct NFTMinted has copy, drop {
        // The Object ID of the NFT 新铸造的NFT的ID
        object_id: ID,
        // The creator of the NFT 新铸造的NFT的创造者
        creator: address,
        // The name of the NFT 新铸造的NFT的名
        name: string::String,
    }

    // ===== Public view functions ===== 公共视图函数

    /// 获取NFT的名称
    /// Get the NFT's `name` 
    public fun name(nft: &DevNetNFT): &string::String {
        &nft.name
    }

    /// 获取NFT的介绍
    /// Get the NFT's `description`
    public fun description(nft: &DevNetNFT): &string::String {
        &nft.description
    }

    /// 获取NFT的链接
    /// Get the NFT's `url`
    public fun url(nft: &DevNetNFT): &Url {
        &nft.url
    }

    // ===== Entrypoints ===== 入口函数

    /// 创建新的NFT
    /// Create a new devnet_nft
    public entry fun mint_to_sender(
        name: vector<u8>,
        description: vector<u8>,
        url: vector<u8>,
        ctx: &mut TxContext
    ) {
        let sender = tx_context::sender(ctx);
        let nft = DevNetNFT {
            id: object::new(ctx),
            name: string::utf8(name),
            description: string::utf8(description),
            url: url::new_unsafe_from_bytes(url)
        };

        event::emit(NFTMinted {
            object_id: object::id(&nft),
            creator: sender,
            name: nft.name,
        });

        transfer::public_transfer(nft, sender);
    }

    /// 转移NFT给新的所有者
    /// Transfer `nft` to `recipient`
    public entry fun transfer(
        nft: DevNetNFT, recipient: address, _: &mut TxContext
    ) {
        transfer::public_transfer(nft, recipient)
    }

    /// 更新NFT的介绍
    /// Update the `description` of `nft` to `new_description`
    public entry fun update_description(
        nft: &mut DevNetNFT,
        new_description: vector<u8>,
        _: &mut TxContext
    ) {
        nft.description = string::utf8(new_description)
    }

    /// 永久删除NFT
    /// Permanently delete `nft`
    public entry fun burn(nft: DevNetNFT, _: &mut TxContext) {
        let DevNetNFT { id, name: _, description: _, url: _ } = nft;
        object::delete(id)
    }
}
