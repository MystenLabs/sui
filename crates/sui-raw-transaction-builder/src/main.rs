use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{CallArg, TransactionData};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    multiaddr::Multiaddr,
    sui_system_state::SUI_SYSTEM_MODULE_NAME,
};
use sui_types::SUI_SYSTEM_PACKAGE_ID;
use move_core_types::ident_str;
use clap::Parser;
use std::path::PathBuf;
use sui_sdk::wallet_context::WalletContext;
use fastcrypto::encoding::{Base64, Encoding};

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
#[clap(name = env!("CARGO_BIN_NAME"))]
struct Args {
    #[clap(long)]
    pub client_config_path: PathBuf,
    #[clap(long)]
    pub sender_address: SuiAddress,
    #[clap(long)]
    pub gas_object_id: ObjectID,
    #[clap(long)]
    pub gas_budget: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let mut builder = ProgrammableTransactionBuilder::new();

    // TODO: change the addresses below to the correct ones.
    let network_address = Multiaddr::try_from("/ip4/0.0.0.0/tcp/1500/http").unwrap();
    let p2p_address = Multiaddr::try_from("/ip4/0.0.0.0/tcp/1500/http").unwrap();
    let primary_address = Multiaddr::try_from("/ip4/0.0.0.0/tcp/1500/http").unwrap();
    let worker_address = Multiaddr::try_from("/ip4/0.0.0.0/tcp/1500/http").unwrap();

    builder.move_call(
        SUI_SYSTEM_PACKAGE_ID,
        SUI_SYSTEM_MODULE_NAME.to_owned(),
        ident_str!("update_validator_next_epoch_network_address").to_owned(),
        vec![],
        vec![
            CallArg::SUI_SYSTEM_MUT,
            CallArg::Pure(bcs::to_bytes(&network_address).unwrap())
        ]
    ).expect("failed to construct network addr update command");

    builder.move_call(
        SUI_SYSTEM_PACKAGE_ID,
        SUI_SYSTEM_MODULE_NAME.to_owned(),
        ident_str!("update_validator_next_epoch_p2p_address").to_owned(),
        vec![],
        vec![
            CallArg::SUI_SYSTEM_MUT,
            CallArg::Pure(bcs::to_bytes(&p2p_address).unwrap())
        ]
    ).expect("failed to construct p2p addr update command");

    builder.move_call(
        SUI_SYSTEM_PACKAGE_ID,
        SUI_SYSTEM_MODULE_NAME.to_owned(),
        ident_str!("update_validator_next_epoch_primary_address").to_owned(),
        vec![],
        vec![
            CallArg::SUI_SYSTEM_MUT,
            CallArg::Pure(bcs::to_bytes(&primary_address).unwrap())
        ]
    ).expect("failed to construct primary addr update command");

    builder.move_call(
        SUI_SYSTEM_PACKAGE_ID,
        SUI_SYSTEM_MODULE_NAME.to_owned(),
        ident_str!("update_validator_next_epoch_worker_address").to_owned(),
        vec![],
        vec![
            CallArg::SUI_SYSTEM_MUT,
            CallArg::Pure(bcs::to_bytes(&worker_address).unwrap())
        ]
    ).expect("failed to construct worker addr update command");

    let pt = builder.finish();

    let wallet_ctx = WalletContext::new(
        &args.client_config_path,
        None,
        None,
    )
    .await?;
    let gas_payment = wallet_ctx.get_object_ref(args.gas_object_id).await?;
    let gas_price = wallet_ctx.get_reference_gas_price().await?;
    let data = TransactionData::new_programmable(args.sender_address, vec![gas_payment], pt, args.gas_budget, gas_price);
    let serialized_data = Base64::encode(bcs::to_bytes(&data)?);
    println!("Serialized transaction data: \n{}", serialized_data);
    Ok(())
}
