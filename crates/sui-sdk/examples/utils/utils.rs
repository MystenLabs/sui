pub mod utils {
    use reqwest::Client;
    use std::collections::HashMap;
    use sui_types::base_types::SuiAddress;
    // use sui_config::{sui_config_dir, SUI_CLIENT_CONFIG}; // uncomment if using a local wallet
    // use sui_sdk::{wallet_context::WalletContext}; // uncomment if using a local wallet
    pub async fn sui_address_for_examples() -> Result<SuiAddress, anyhow::Error> {
        // If there is an existing wallet you want to use then the following code can be uncommented

        // let wallet_conf = sui_config_dir()?.join(SUI_CLIENT_CONFIG);
        // let mut wallet =
        //     WalletContext::new(&wallet_conf, Some(std::time::Duration::from_secs(60)), None).await?;
        // let active_address = wallet.active_address()?;

        // If a wallet does exist, then comment out the code after these comments and uncomment the code above
        // Generate a random Sui Address, and add some coins to it. Request tokens (coins) from the faucet JSON RPC API, which requires a JSON body like this
        // "FixedAmountRequest" : {
        //    "recipient": "SUI_ADDRESS"
        // }

        let active_address: SuiAddress = SuiAddress::random_for_testing_only();
        request_tokens_from_faucet(active_address).await?;
        Ok(active_address)
    }

    pub async fn request_tokens_from_faucet(address: SuiAddress) -> Result<(), anyhow::Error> {
        let mut map = HashMap::new();
        let mut recipient = HashMap::new();
        recipient.insert("recipient", address.to_string());
        map.insert("FixedAmountRequest", recipient);

        // make the request to the faucet JSON RPC API for coins
        let client = Client::new();
        let _res = client
            .post("http://127.0.0.1:9123/gas")
            .header("Content-Type", "application/json")
            .json(&map)
            .send()
            .await?;
        Ok(())
    }
}
