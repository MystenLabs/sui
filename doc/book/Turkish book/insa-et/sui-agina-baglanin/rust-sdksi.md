# Rust SDK'sı

### Genel Bakış

[Sui SDK](https://github.com/MystenLabs/sui/tree/main/crates/sui-sdk), [Sui Devnet](https://docs.sui.io/devnet/build/devnet) ve [Sui Full node](https://docs.sui.io/devnet/build/fullnode) ile etkileşim kurmak için kullanabileceğiniz Rust dili JSON-RPC wrapper ve kripto yardımcı programlarından oluşan bir koleksiyondur.

``[`SuiClient`](https://docs.sui.io/devnet/build/cli-client), bir HTTP veya WebSocket client (`SuiClient::new`) oluşturmak için kullanılabilir. Mevcut yöntemlerin listesi için [JSON-RPC](https://docs.sui.io/devnet/build/json-rpc#sui-json-rpc-methods) dokümanımıza bakın.

> _Not:_ [_Sui 0.6.0 sürümünden_](https://github.com/MystenLabs/sui/releases/tag/devnet-0.6.0) _itibaren WebSocket istemcisi_ [_yalnızca abonelik_](https://docs.sui.io/devnet/build/event\_api#subscribe-to-sui-events) _içindir; diğer API yöntemleri için HTTP istemcisini kullanın._

### Kaynakça <a href="#references" id="references"></a>

Önemli Sui projeleri için `rustdoc` çıktısını şu adreste bulabilirsiniz:

* Sui blockchain - [https://mystenlabs.github.io/sui/](https://mystenlabs.github.io/sui/)
* Narwhal ve Bullshark konsensüs motoru - [https://mystenlabs.github.io/narwhal/](https://mystenlabs.github.io/narwhal/)
* Mysten Labs altyapısı - [https://mystenlabs.github.io/mysten-infra/](https://mystenlabs.github.io/mysten-infra/)

### Yapılandırma <a href="#configuration" id="configuration"></a>

`Cargo.toml` dosyanıza `sui-sdk` crate'ini şu şekilde ekleyin:

```
[dependencies]
sui-sdk = { git = "https://github.com/MystenLabs/sui" }
```

Eğer devnet'e bağlanıyorsanız, bunun yerine `devnet` şubesini kullanın:

```
[dependencies]
sui-sdk = { git = "https://github.com/MystenLabs/sui", branch = "devnet" }
```

### Örnekler <a href="#examples" id="examples"></a>

#### Örnek 1 - Bir adrese ait tüm nesneleri alın

Bu, `"0xec11cad080d0496a53bafcea629fcbcfff2a9866"` adresine ait nesne özetlerinin bir listesini yazdıracaktır:

```
use std::str::FromStr;
use sui_sdk::types::base_types::SuiAddress;
use sui_sdk::SuiClient;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let sui = SuiClient::new("https://fullnode.devnet.sui.io:443", None, None).await?;
    let address = SuiAddress::from_str("0xec11cad080d0496a53bafcea629fcbcfff2a9866")?;
    let objects = sui.read_api().get_objects_owned_by_address(address).await?;
    println!("{:?}", objects);
    Ok(())
}
```

Sui Devnet Full node kullanıyorsanız sonucu [Sui Gezgini](https://explorer.sui.io/) ile doğrulayabilirsiniz..

#### Örnek 2 - İşlem oluşturma ve yürütme <a href="#example-2---create-and-execute-transaction" id="example-2---create-and-execute-transaction"></a>

Sui Devnet Full node kullanarak Sui'de bir işlem gerçekleştirmek için bu örneği kullanın:

```
use std::str::FromStr;
use sui_sdk::{
    crypto::{FileBasedKeystore, Keystore},
    types::{
        base_types::{ObjectID, SuiAddress},
        crypto::Signature,
        messages::Transaction,
    },
    SuiClient,
};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let sui = SuiClient::new("https://fullnode.devnet.sui.io:443", None, None).await?;
    // Load keystore from ~/.sui/sui_config/sui.keystore
    let keystore_path = match dirs::home_dir() {
        Some(v) => v.join(".sui").join("sui_config").join("sui.keystore"),
        None => panic!("Cannot obtain home directory path"),
    };

    let my_address = SuiAddress::from_str("0x47722589dc23d63e82862f7814070002ffaaa465")?;
    let gas_object_id = ObjectID::from_str("0x273b2a83f1af1fda3ddbc02ad31367fcb146a814")?;
    let recipient = SuiAddress::from_str("0xbd42a850e81ebb8f80283266951d4f4f5722e301")?;

    // Create a sui transfer transaction
    let transfer_tx = sui
        .transaction_builder()
        .transfer_sui(my_address, gas_object_id, 1000, recipient, Some(1000))
        .await?;

    // Sign transaction
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let signature = keystore.sign_secure(&my_address, &transfer_tx, Intent::default())?;
    
    // Execute the transaction
    let transaction_response = sui
        .quorum_driver()
        .execute_transaction(Transaction::from_data(transfer_tx, Intent::default(), signature))

    println!("{:?}", transaction_response);

    Ok(())
}
```

#### Örnek 3 - Etkinlik aboneliği <a href="#example-3---event-subscription" id="example-3---event-subscription"></a>

[Etkinliklere abone olmak](https://docs.sui.io/devnet/build/event\_api#subscribe-to-sui-events) için WebSocket istemcisini kullanın.

```
use futures::StreamExt;
use sui_sdk::rpc_types::SuiEventFilter;
use sui_sdk::SuiClient;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let sui = SuiClient::new("https://fullnode.devnet.sui.io:443", Some("ws://127.0.0.1:9001"), None).await?;
    let mut subscribe_all = sui.event_api().subscribe_event(SuiEventFilter::All(vec![])).await?;
    loop {
        println!("{:?}", subscribe_all.next().await);
    }
}
```

> Not: Olay abonelik hizmeti için bir tam düğüme bağlanmanız gerekecektir, bir Sui Tam Node çalıştırmak istiyorsanız [Tam node kurulumu](https://docs.sui.io/devnet/build/fullnode#fullnode-setup) bölümüne bakın.

### Daha büyük örnekler <a href="#larger-examples" id="larger-examples"></a>

[Tic Tac Toe](https://github.com/MystenLabs/sui/tree/main/crates/sui-sdk) örneği için Sui Rust SDK README'ye bakın.
