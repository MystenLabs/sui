// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use fastpay_core::client::ClientState;
use fastx_types::{
    base_types::*,
    messages::{Address, CertifiedOrder, OrderKind},
};

use move_core_types::language_storage::TypeTag;
use move_core_types::{identifier::Identifier, transaction_argument::TransactionArgument};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, read_to_string, File, OpenOptions},
    io::{BufReader, BufWriter, Write},
    iter::FromIterator,
};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthorityConfig {
    #[serde(
        serialize_with = "address_as_hex",
        deserialize_with = "address_from_hex"
    )]
    pub address: FastPayAddress,
    pub host: String,
    pub base_port: u32,
    pub database_path: String,
}

impl AuthorityConfig {
    pub fn print(&self) {
        let data = serde_json::to_string(self).unwrap();
        println!("{}", data);
    }
}

#[derive(Serialize, Deserialize)]
pub struct AuthorityServerConfig {
    pub authority: AuthorityConfig,
    pub key: KeyPair,
}

impl AuthorityServerConfig {
    pub fn read(path: &str) -> Result<Self, std::io::Error> {
        let data = fs::read(path)?;
        Ok(serde_json::from_slice(data.as_slice())?)
    }

    pub fn write(&self, path: &str) -> Result<(), std::io::Error> {
        let file = OpenOptions::new().create(true).write(true).open(path)?;
        let mut writer = BufWriter::new(file);
        let data = serde_json::to_string_pretty(self).unwrap();
        writer.write_all(data.as_ref())?;
        writer.write_all(b"\n")?;
        Ok(())
    }
}

pub struct CommitteeConfig {
    pub authorities: Vec<AuthorityConfig>,
}

impl CommitteeConfig {
    pub fn read(path: &str) -> Result<Self, std::io::Error> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let stream = serde_json::Deserializer::from_reader(reader).into_iter();
        Ok(Self {
            authorities: stream.filter_map(Result::ok).collect(),
        })
    }

    pub fn write(&self, path: &str) -> Result<(), std::io::Error> {
        let file = OpenOptions::new().create(true).write(true).open(path)?;
        let mut writer = BufWriter::new(file);
        for config in &self.authorities {
            serde_json::to_writer(&mut writer, config)?;
            writer.write_all(b"\n")?;
        }
        Ok(())
    }

    pub fn voting_rights(&self) -> BTreeMap<AuthorityName, usize> {
        let mut map = BTreeMap::new();
        for authority in &self.authorities {
            map.insert(authority.address, 1);
        }
        map
    }
}

#[serde_as]
#[derive(Serialize, Deserialize)]
pub struct UserAccount {
    #[serde(
        serialize_with = "address_as_hex",
        deserialize_with = "address_from_hex"
    )]
    pub address: FastPayAddress,
    pub key: KeyPair,
    pub object_refs: BTreeMap<ObjectID, ObjectRef>,
    pub gas_object_ids: BTreeSet<ObjectID>, // Every id in gas_object_ids should also be in object_ids.
    #[serde_as(as = "Vec<(_, _)>")]
    pub certificates: BTreeMap<TransactionDigest, CertifiedOrder>,
}

impl UserAccount {
    pub fn new(object_refs: Vec<ObjectRef>, gas_object_ids: Vec<ObjectID>) -> Self {
        let (address, key) = get_key_pair();
        let object_refs = object_refs
            .into_iter()
            .map(|object_ref| (object_ref.0, object_ref))
            .collect();
        let gas_object_ids = BTreeSet::from_iter(gas_object_ids);
        Self {
            address,
            key,
            object_refs,
            gas_object_ids,
            certificates: BTreeMap::new(),
        }
    }
}

pub fn transaction_args_from_str<'de, D>(
    deserializer: D,
) -> Result<Vec<TransactionArgument>, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;

    let tokens = s.split(',');

    let result: Result<Vec<_>, _> = tokens
        .map(|tok| move_core_types::parser::parse_transaction_argument(tok.trim()))
        .collect();
    result.map_err(serde::de::Error::custom)
}
#[derive(Serialize, Deserialize)]
pub struct MoveCallConfig {
    /// Object ID of the package, which contains the module
    pub package_obj_id: ObjectID,
    /// The name of the module in the package
    pub module: Identifier,
    /// Function name in module
    pub function: Identifier,
    /// Function name in module
    pub type_args: Vec<TypeTag>,
    /// Object args object IDs
    pub object_args_ids: Vec<ObjectID>,

    /// Pure arguments to the functions, which conform to move_core_types::transaction_argument
    /// Special case formatting rules:
    /// Use one string with CSV token embedded, for example "54u8,0x43"
    /// When specifying FastX addresses, specify as vector. Example x\"01FE4E6F9F57935C5150A486B5B78AC2B94E2C5CD9352C132691D99B3E8E095C\"
    #[serde(deserialize_with = "transaction_args_from_str")]
    pub pure_args: Vec<TransactionArgument>,
    /// ID of the gas object for gas payment, in 20 bytes Hex string
    pub gas_object_id: ObjectID,
    /// Gas budget for this call
    pub gas_budget: u64,
}

impl MoveCallConfig {
    pub fn read(path: &str) -> Result<Self, std::io::Error> {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(path)?;
        let reader = BufReader::new(file);
        Ok(serde_json::from_reader(reader)?)
    }

    pub fn write(&self, path: &str) -> Result<(), std::io::Error> {
        let file = OpenOptions::new().write(true).open(path)?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer(&mut writer, self)?;
        writer.write_all(b"\n")?;
        Ok(())
    }
}

pub struct AccountsConfig {
    accounts: BTreeMap<FastPayAddress, UserAccount>,
}

impl AccountsConfig {
    pub fn get(&self, address: &FastPayAddress) -> Option<&UserAccount> {
        self.accounts.get(address)
    }

    pub fn insert(&mut self, account: UserAccount) {
        self.accounts.insert(account.address, account);
    }

    pub fn num_accounts(&self) -> usize {
        self.accounts.len()
    }

    pub fn nth_account(&self, n: usize) -> Option<&UserAccount> {
        self.accounts.values().nth(n)
    }

    pub fn find_account(&self, object_id: &ObjectID) -> Option<&UserAccount> {
        self.accounts
            .values()
            .find(|acc| acc.object_refs.contains_key(object_id))
    }
    pub fn accounts_mut(&mut self) -> impl Iterator<Item = &mut UserAccount> {
        self.accounts.values_mut()
    }

    pub fn addresses(&mut self) -> impl Iterator<Item = &FastPayAddress> {
        self.accounts.keys()
    }

    pub fn update_from_state<A>(&mut self, state: &ClientState<A>) {
        let account = self
            .accounts
            .get_mut(&state.address())
            .expect("Updated account should already exist");
        account.object_refs = state.get_object_refs();
        account.certificates = state.get_all_certified_orders();
    }

    pub fn update_for_received_transfer(&mut self, certificate: CertifiedOrder) {
        match &certificate.order.kind {
            OrderKind::Transfer(transfer) => {
                if let Address::FastPay(recipient) = &transfer.recipient {
                    if let Some(config) = self.accounts.get_mut(recipient) {
                        config
                            .certificates
                            .entry(certificate.order.digest())
                            .or_insert(certificate);
                    }
                }
            }
            OrderKind::Publish(_) | OrderKind::Call(_) => {
                unimplemented!("update_for_received_transfer of Call or Publish")
            }
        }
    }

    pub fn read_or_create(path: &str) -> Result<Self, std::io::Error> {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(path)?;
        let reader = BufReader::new(file);
        let stream = serde_json::Deserializer::from_reader(reader).into_iter();
        Ok(Self {
            accounts: stream
                .filter_map(Result::ok)
                .map(|account: UserAccount| (account.address, account))
                .collect(),
        })
    }

    pub fn write(&self, path: &str) -> Result<(), std::io::Error> {
        let file = OpenOptions::new().write(true).open(path)?;
        let mut writer = BufWriter::new(file);
        for account in self.accounts.values() {
            serde_json::to_writer(&mut writer, account)?;
            writer.write_all(b"\n")?;
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
pub struct InitialStateConfigEntry {
    pub address: FastPayAddress,
    pub object_refs: Vec<ObjectRef>,
}
#[derive(Serialize, Deserialize)]
pub struct InitialStateConfig {
    pub config: Vec<InitialStateConfigEntry>,
}

impl InitialStateConfig {
    pub fn new() -> Self {
        Self { config: Vec::new() }
    }

    pub fn read(path: &str) -> Result<Self, anyhow::Error> {
        let raw_data: String = read_to_string(path)?.parse()?;

        Ok(toml::from_str(&raw_data)?)
    }

    pub fn write(&self, path: &str) -> Result<(), std::io::Error> {
        let config = toml::to_string(self).unwrap();

        fs::write(path, config).expect("Unable to write to initial config file");
        Ok(())
    }
}

impl Default for InitialStateConfig {
    fn default() -> Self {
        Self::new()
    }
}
