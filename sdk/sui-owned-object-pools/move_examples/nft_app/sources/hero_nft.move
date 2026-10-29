module cms::hero_nft {

    // Std library imports
    use std::string::{String};

    // Sui imports
    use sui::tx_context::{TxContext};
    use sui::object::{Self, UID};

    // Module dependency
    use cms::genesis::{AdminCap, SharedItem};
    use sui::transfer;

    /// The Hero NFT struct
    struct Hero has key, store 
    {
        id: UID,
        name: String,       
        tier: String,       
        star: u8,    
        image_url: String,  
    }

    /// Create an NFT Hero 
    public fun mint_hero(
        _: &mut AdminCap, name: String, tier: String,
        star: u8, image_url: String,
        ctx: &mut TxContext
    ): Hero {
        let hero_nft = Hero {
            id: object::new(ctx),
            name,
            tier,
            star,
            image_url
        };

        // returned Hero 
        hero_nft
    }

    public fun mint_immutable_hero(
        _: &mut AdminCap, 
        name: String, 
        tier: String,
        star: u8, image_url: String,
        ctx: &mut TxContext
    ) {
        let hero = mint_hero(_, name, tier, star, image_url, ctx);
        transfer::public_freeze_object(hero);
    }

    /// Updates the star rating of a Hero
    public fun update_stars(hero: &mut Hero, stars_to_upg: u8) 
    {
        hero.star = stars_to_upg;
    }

    /// Updates the image_url of a Hero
    public fun update_image_url(hero: &mut Hero, image_url: String) 
    {
        hero.image_url = image_url;
    }

    /// Unpacking the hero object to delete it.
    /// Since this is a mutating action it can't be performed by the owner if exported (unless this is desired)
    public fun delete_hero(
        hero: Hero
    ) {
        let Hero {
            id, 
            name: _, 
            tier: _, 
            star: _, 
            image_url: _,
        } = hero;

        object::delete(id);
    }

    public fun process_shared_item(
        _shared_item: &mut SharedItem, 
    ) {
        // do nothing, we just want to test that we can execute
        // a transaction block containing a shared item
    }
}