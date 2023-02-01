//// An example of an authenticator-NFT pattern
//// The protocol has 3 steps:
//// 1. The NFT owner asks a web2 server for a random public_auth_token and secret_auth_token pair.
//// 2. The NFT owner calls authenticate_action method of this Move contract, disclosing his public_auth_token.
//// 3. The server sees ActionViaNFTEvent with public_auth_token. The user logs in with his secret_auth_token part.  

module nfts::auth_nft {
  
    use sui::url::{Self, Url};
    use std::string;
    use sui::object::{Self, ID, UID};
    use sui::event;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    /// An example NFT similar to DevNetNFT
    struct ActionNFT has key, store {
        id: UID,
        name: string::String,
        description: string::String,
        url: Url,
        actions: vector<Action>,
    }

    /// An action to be executed against a web2 endpoint specified with its Url
    struct Action has store, drop {
        endpoint: Url,
        name: string::String,
    }

    /// An event to prove the possesion of particular NFT and knowledge of an arbitrary public_auth_token
    struct ActionViaNFTEvent has copy, drop {
        object_id: ID,
        public_auth_token : string::String,
    }

    /// Create a new ActionNFT
    public entry fun mint(
        name: vector<u8>,
        description: vector<u8>,
        url: vector<u8>,
        endpoint : vector<u8>,
        action_name: vector<u8>,
        ctx: &mut TxContext
    ) {
        //Initialize an empty array for actions
        let actions = std::vector::empty<Action>();

        // TODO: Allow to add arbitrary many actions
        // Now only one is added for the proof-of-concept
        std::vector::push_back(&mut actions, 
        Action {
             endpoint: url::new_unsafe_from_bytes(endpoint), 
             name: string::utf8(action_name)
        });

        // Create the ActionNFT struct
        // Filled with some metadata and actions
        let nft = ActionNFT {
            id: object::new(ctx),
            name: string::utf8(name),
            description: string::utf8(description),
            url: url::new_unsafe_from_bytes(url),
            actions,
        };

        // Transfer the minted ActionNFT to the sender
        let sender = tx_context::sender(ctx);
        transfer::transfer(nft, sender);
    }

    /// A method emits an event to prove possesion of NFT at the moment, knowledge of public_auth_token
    public entry fun authenticate_action(nft: &ActionNFT, token: vector<u8>){
        event::emit(ActionViaNFTEvent {
            object_id: object::uid_to_inner(&nft.id),
            public_auth_token : string::utf8(token),
        });
    }

}
