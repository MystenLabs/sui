module aptos_token::simple_coin {
    use std::signer;
    use std::string;
    use aptos_framework::coin::{Self, Coin};
    use aptos_framework::account;

    /// Error codes
    const ENOT_ADMIN: u64 = 1;
    const EINSUFFICIENT_BALANCE: u64 = 2;
    const EALREADY_INITIALIZED: u64 = 3;

    /// The coin type representing our custom token
    struct SimpleCoin has key {}

    /// Capability to mint and burn
    struct Capabilities has key {
        mint_cap: coin::MintCapability<SimpleCoin>,
        burn_cap: coin::BurnCapability<SimpleCoin>,
        freeze_cap: coin::FreezeCapability<SimpleCoin>,
    }

    /// Initialize the coin module
    /// This should be called by the module publisher
    public entry fun initialize(
        account: &signer,
    ) {
        let account_addr = signer::address_of(account);

        // Ensure not already initialized
        assert!(!exists<Capabilities>(account_addr), EALREADY_INITIALIZED);

        // Initialize coin with metadata
        let (burn_cap, freeze_cap, mint_cap) = coin::initialize<SimpleCoin>(
            account,
            string::utf8(b"Simple Coin"),
            string::utf8(b"SMPL"),
            8, // decimals
            true, // monitor_supply
        );

        // Store capabilities
        move_to(account, Capabilities {
            mint_cap,
            burn_cap,
            freeze_cap,
        });
    }

    /// Register account to receive SimpleCoin
    public entry fun register(account: &signer) {
        coin::register<SimpleCoin>(account);
    }

    /// Mint new coins (admin only)
    public entry fun mint(
        admin: &signer,
        recipient: address,
        amount: u64,
    ) acquires Capabilities {
        let admin_addr = signer::address_of(admin);
        assert!(exists<Capabilities>(admin_addr), ENOT_ADMIN);

        let capabilities = borrow_global<Capabilities>(admin_addr);
        let minted_coins = coin::mint<SimpleCoin>(amount, &capabilities.mint_cap);

        coin::deposit<SimpleCoin>(recipient, minted_coins);
    }

    /// Burn coins from sender's account
    public entry fun burn(
        account: &signer,
        amount: u64,
    ) acquires Capabilities {
        let account_addr = signer::address_of(account);
        let coins_to_burn = coin::withdraw<SimpleCoin>(account, amount);

        // Get burn capability from admin (in production, store admin address)
        let capabilities = borrow_global<Capabilities>(@aptos_token);
        coin::burn<SimpleCoin>(coins_to_burn, &capabilities.burn_cap);
    }

    /// Transfer coins between accounts
    public entry fun transfer(
        from: &signer,
        to: address,
        amount: u64,
    ) {
        let coins = coin::withdraw<SimpleCoin>(from, amount);
        coin::deposit<SimpleCoin>(to, coins);
    }

    /// Get balance of an account
    #[view]
    public fun balance(account_addr: address): u64 {
        coin::balance<SimpleCoin>(account_addr)
    }

    /// Get total supply
    #[view]
    public fun total_supply(): u128 {
        let supply = coin::supply<SimpleCoin>();
        *std::option::borrow(&supply)
    }

    #[test(admin = @aptos_token, user = @0x456)]
    public fun test_mint_transfer(admin: &signer, user: &signer) acquires Capabilities {
        use aptos_framework::aptos_account;

        // Setup
        let admin_addr = signer::address_of(admin);
        let user_addr = signer::address_of(user);

        account::create_account_for_test(admin_addr);
        account::create_account_for_test(user_addr);

        // Initialize
        initialize(admin);

        // Register user
        register(user);

        // Mint to user
        mint(admin, user_addr, 1000);

        // Check balance
        assert!(balance(user_addr) == 1000, 0);

        // Transfer
        transfer(user, admin_addr, 300);

        assert!(balance(user_addr) == 700, 1);
        assert!(balance(admin_addr) == 300, 2);
    }

    #[test(admin = @aptos_token)]
    public fun test_burn(admin: &signer) acquires Capabilities {
        let admin_addr = signer::address_of(admin);
        account::create_account_for_test(admin_addr);

        initialize(admin);
        mint(admin, admin_addr, 1000);

        let initial_supply = total_supply();
        burn(admin, 300);

        assert!(balance(admin_addr) == 700, 0);
        assert!(total_supply() == initial_supply - 300, 1);
    }
}
