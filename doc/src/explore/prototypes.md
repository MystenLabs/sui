---
title: Sui Prototypes
---

Here are two short gaming pre-alpha prototypes that demonstrate the speed, scalability, and rich interactions made possible with mutable, expressive NFTs. The first prototype [Sui Monstars](https://sui.io/monstar), is playable now.


## Sui and gaming

Gaming as one of the first verticals for rapid web3 adoption is a popular talking point. However, existing web3 games are arguably regarded more as investments than games, with user retention impacted by market conditions rather than the games themselves.

So what is missing from existing web3 games? Firstly, a successful web3 game must offer an experience wholly different from any web1 or web2 games. To truly shine, web3 games must, in a meaningful way, take advantage of the benefits of fully on-chain, dynamic and composable digital assets with verifiable ownership. These features can power incredible and imaginative gameplay and ecosystems, creating immense value and engagement.

Secondly, great games require experienced game developers and builders–people who know how to build games and create fun, user-centric experiences. There is a wealth of talent eager to build in web3, but its creativity has been hindered by platform limitations and the pains of learning a new programming language.

With Sui, we believe game developers should not be limited by the platform performance or fees, and they should be able to create whatever experience they imagine. Importantly, developing great games should not require game developers to also be experts in writing smart contracts. Rather they should focus on what they are good at, building cool games for gamers.


## Smart contracts optional

[Move](https://github.com/MystenLabs/awesome-move/blob/main/README.md) is simply awesome: it’s safe, expressive and immune from reentrancy; but Move expertise is not required to build meaningful experiences on Sui. To make it easy for developers and creators to start using Sui for gaming, we will be releasing gaming SDKs that address common use cases and game asset-related features.


## How we did it

Created by game development studio GenITeam, these prototypes use both the Unity SDK and Sui [APIs](https://playground.open-rpc.org/?uiSchema%5BappBar%5D%5Bui:splitView%5D=false&schemaUrl=https://raw.githubusercontent.com/MystenLabs/sui/main/sui/open_rpc/spec/openrpc.json&uiSchema%5BappBar%5D%5Bui:input%5D=false).

GenITeam’s developers who worked on this collaboration are neither smart contract nor Move developers. Based on their input, we created a data model and shared simple APIs. With these APIs, Geniteam was able to mint fully on-chain NFTs that are mutable, own other on-chain assets, and freely transfer to other applications.

This proof of concept build is meant to demonstrate the capabilities for game developers unlocked through Sui. We look forward to seeing what the creative minds in the gaming community come up with as we unveil additional capabilities in the upcoming months. With each bug fixed we learned insights on what game developers look for in a SDK. Sui is committed to building SDKs that are accessible for all levels of developers with varying degrees of smart contracts expertise.

Here are examples of APIs we shared, along with the smart contracts to create and update monster (named Monstars in the prototype):


### API Move call - Create Monster

POST `/call` with body:

```
    {
       "sender": "{{owner}}",
       "packageObjectId": "0x2",
       "module": "Geniteam",
       "function": "create_monster",
       "args": [
           "0x{{player_id}}",
           "0x{{farm_id}}",
           "0x{{pet_monsters}}",
           {{monster_name}},
           {{monster_img_index}},
           {{breed}},
           {{monster_affinity}},
           {{monster_description}}
       ],
       "gasObjectId": "{{gas_object_id}}",
       "gasBudget": 2000
```


### API Move call - Update Monster

POST `/call` with body:

```
    {
       "sender": "{{owner}}",
       "packageObjectId": "0x2",
       "module": "Geniteam",
       "function": "update_monster_stats",
       "args": [
           "0x{{player_id}}",
           "0x{{farm_id}}",
           "0x{{pet_monsters}}",
           "0x{{monster_id}}",
           {{monster_level}},
           {{hunger_level}},
           {{affection_level}},
           {{buddy_level}}
       ],
       "gasObjectId": "{{gas_object_id}}",
       "gasBudget": 2000
```

### API Move call - Read Monster Data

```
GET /object_info?objectId={{monster_id}}
```


### Smart contract: Create Monster

```
   struct Monster has key, store {
        id: VersionedID,
        monster_name: String,
        monster_img_index: u64,
        breed: u8,
        monster_affinity: u8,
        monster_description: String,
        monster_level: u64,
        monster_xp: u64,
        hunger_level: u64,
        affection_level: u64,
        buddy_level: u8,

        // ID of the applied cosmetic at this slot
        applied_monster_cosmetic_0_id: Option<ID>,
        // ID of the applied cosmetic at this slot
        applied_monster_cosmetic_1_id: Option<ID>,
    }

    // Create a Monster and add it to the Farm's collection of Monsters
    public(script) fun create_monster(_player: &mut Player,
                              farm: &mut Farm,
                              pet_monsters_c: &mut Collection::Collection,
                              monster_name: vector<u8>,
                              monster_img_index: u64,
                              breed: u8,
                              monster_affinity: u8,
                              monster_description: vector<u8>,
                              ctx: &mut TxContext
    ) {

        let monster = create_monster_(
            monster_name,
            monster_img_index,
            breed,
            monster_affinity,
            monster_description,
            ctx
        );

        // Check if this is the right collection
        assert!(*&farm.pet_monsters_id == *ID::id(pet_monsters_c), EMONSTER_COLLECTION_NOT_OWNED_BY_FARM);


        // Add it to the collection
        Collection::add(pet_monsters_c, monster);
    }

    // Creates a basic Monster object
    public(script) fun create_monster_(
        monster_name: vector<u8>,
        monster_img_index: u64,
        breed: u8,
        monster_affinity: u8,
        monster_description: vector<u8>,
        ctx: &mut TxContext
    ): Monster {

        Monster {
            id: TxContext::new_id(ctx),
            monster_name: ASCII::string(monster_name),
            monster_img_index,
            breed,
            monster_affinity,
            monster_description: ASCII::string(monster_description),
            monster_level: 0,
            monster_xp: 0,
            hunger_level: 0,
            affection_level: 0,
            buddy_level: 0,
            applied_monster_cosmetic_0_id: Option::none(),
            applied_monster_cosmetic_1_id: Option::none(),
        }
    }
```

###  Smart contract: Update Monster

```
    // Update the attributes of a monster
    public(script) fun update_monster_stats(
        _player: &mut Player,
        _farm: &mut Farm,
        _pet_monsters: &mut Collection::Collection,
        self: &mut Monster,
        monster_level: u64,
        hunger_level: u64,
        affection_level: u64,
        buddy_level: u8,
        _ctx: &mut TxContext
    ) {
        self.monster_level = monster_level;
        self.hunger_level = hunger_level;
        self.affection_level = affection_level;
        self.buddy_level = buddy_level;
    }
```

## Protype 1 Sui Monstar

The first prototype is [Sui Monstar](https://sui.io/monstar), a pet simulation game.

Gameplay:

* Play, feed and dress up your canine and feline friends.
* Evolve your pets with affinity runes!
* Decorate your farm.
* Raise your farm and pet levels through gameplay and interactions.

In Sui Monstars, capture cute monstars and watch them get closer to you as you feed and interact with them. These monstars, your farm, and accessories are all NFTs on-chain. As you play through the game, attributes such as health, friendliness, and accessories are all updated live.

<section class="sui-dev-video">
   <iframe id="ytplayer" type="text/html" src="https://www.youtube.com/embed/sAMT5x8W3B8?autoplay=0"  frameborder="0"></iframe>
</section>

![Update NFT properties](../../static/nft-properties.png "Equip elemental runes to your Monstar")
*Equip elemental runes to your Monstar and watch your NFT evolve with updated properties*

That’s not all! As your Monstars become stronger, you can use them to help you battle…in the next prototype>>>

## Prototype 2 Sui Battler

Welcome to Sui Battler, where your cute monstars transform into warriors!

Gameplay:

* Battle waves of enemies and gain experience and power-ups.
* Get help from your own pet from Sui Monstars.
* Evolve your pet in Sui Monstars and unlock special battle abilities.
* Your monstars record the history of your battle on-chain!

![Unlock special abilities](../../static/special-abilities.png "Evolve your Monstars")
*Evolve your Monstars to unlock special abilities*

## Why this matters

* Mutable NFTs means richer and more creative gameplay. No more complicated workarounds or burning NFTs, losing all your data and history, in order to “modify” NFTs.
* Usability-focused APIs make building on Sui easy.
* Unparalleled scalability and instant settlement mean changes, asset status, balance and ownerships can happen instantly live along with gameplay. No more lag or workarounds.
* Creativity is the limit. Creators can freely use their assets across various applications and games.
* Fully on-chain, composable NFTs with rich history make possible the next generation of game economies.

## Further reading

* Check out Sui [APIs](https://playground.open-rpc.org/?uiSchema%5BappBar%5D%5Bui:splitView%5D=false&schemaUrl=https://raw.githubusercontent.com/MystenLabs/sui/main/sui/open_rpc/spec/openrpc.json&uiSchema%5BappBar%5D%5Bui:input%5D=false).
* Learn about Sui [objects](../build/objects.md).
