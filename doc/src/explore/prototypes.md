---
title: Sui Prototypes
---

Here are two short gaming prototypes that demonstrate the speed, scalability, and rich interactions made possible with mutable, expressive NFTs:



* [Sui Monstar](https://sui.io/sui_monstar)
* [Sui Battler](https:/sui.io/sui_battler)


## Sui and Gaming

Gaming as one of the first verticals for rapid web3 adoption is a popular talking point. However, existing web3 games are arguably regarded more as investments than games, with user retention impacted by market conditions rather than the games themselves.

To have good games, you need good game developers and builders, people who know how to build games and create fun, user-centric experiences. There is a wealth of talent eager to build in web3, but its creativity has been hindered by platform limitations and the pains of learning a new programming language.

With Sui, we believe game developers should not be limited by the platform performance or fees, and they should be able to create whatever experience they imagine. Importantly, developing great games should not require game dev's to be experts in writing smart contracts. Rather they should focus on what they are good at, building cool games for gamers


## Smart Contracts Optional

[Move](https://github.com/MystenLabs/awesome-move/blob/main/README.md) is simply awesome: it’s safe, expressive and immune from reentrancy; but move expertise is not required to build meaningful experiences on Sui. To make it easy for developers and creators to start using Sui for gaming, we will be releasing gaming SDKs that address common use cases and game asset-related features.


## How We Did It

To create these prototypes, we worked with game development studio Geniteam, who built the prototypes with the Unity SDK along with Sui [APIs](https://app.swaggerhub.com/apis/MystenLabs/sui-api/0.1).

Geniteam developers that worked on this collaboration are not smart contract or move developers. With this project, we started gathering data on what is the best way to design SDKs that make it easy to start building on Sui.

Once Geniteam communicated their idea with us, we created the proposed data model and shared simple APIs. With these APIs, Geniteam was able to mint fully on-chain NFTs that are able to mutate, own other on-chain assets, and freely transfer to other applications. Gameplay is then powered by APIs calls that allow them to read and write to update the NFTs. \


Here are the three APIs Geniteam used, along with the smart contracts to create and update monster (named MonStars in the prototype).


### API Move Call - Create Monster

POST `/call `with body:


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



### <code>} \
 \
<strong>API Move Call - Update Monster</strong></code>

POST `/call `with body:


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



### `} \
 \
`API Move Call - Read Monster Data

GET /object_info?objectId=<code><em>{{monster_id}}</em> \
</code>


### Smart Contract: Create Monster


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
```



```
    // Create a Monster and add it to the Farm's collection of Monsters
    public fun create_monster(_player: &mut Player,
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
    public fun create_monster_(
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



###  \
**Smart Contract: Update Monster** \



```
    // Update the attributes of a monster
    public fun update_monster_stats(
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


 \



## Protype 1 Sui Monstar

The first prototype is Sui Monstar[link], a pet simulation game.

Gameplay:



* Play, feed and dress up your canine and feline friends.
* Evolve your pets with affinity runes!
* Decorate your farm.
* Raise your farm and pet levels through gameplay and interactions.

In Sui Monstar, capture cute monstars and watch them get closer to you as you feed and interact with them. These monstars, your farm, and accessories are all NFTs on-chain. As you play through the game, attributes such as health, friendliness, and accessories are all updated live.

[insert images from explorer of a dog and then an evolved dog, highlighting that it’s still the same NFT]

That’s not all, as your Monstar become stronger, you can use them to help you battle…in the next prototype>>>


## Prototype 2 Sui Battler

Welcome to Sui Battler [link], where your cute monstars transform into warriors!

Gameplay:



* Battle waves of enemies and gain experience and power-ups.
* Get help from your own pet in Sui Monstar.
* Evolve your pet in Sui Monstar and unlock special battle abilities.
* Your monstars record the history of your battle on-chain!

[insert image of a monstar card along with the matching explorer view, highlighting experience and wave count]

[insert image of an evolved white fire cat helping in-game]


## Why This Matters



* Mutable NFTs means richer and more creative gameplay. No more complicated workarounds or burning NFTs, losing all your data and history, in order to “modify” NFTs.
* Usability-focused APIs make building on Sui easy.
* Unparalleled scalability and instant settlement mean changes, asset status, balance and ownerships can happen instantly live along with gameplay. No more lag or workarounds.
* Creativity is the limit. Creators can freely use their assets across various applications and games.


## Further reading



* See the entire unity project here[link to github].
* Check out Sui [APIs](https://app.swaggerhub.com/apis/MystenLabs/sui-api/0.1).
* Learn about Sui [objects](https://github.com/MystenLabs/sui/blob/main/doc/src/build/objects.md).
