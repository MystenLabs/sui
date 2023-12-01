use std::{collections::BTreeMap, fs, io::BufReader, path::PathBuf};

use sui_single_node_benchmark::mock_account::Account;
use sui_types::{
    base_types::{ObjectID, ObjectRef, SuiAddress},
    object::Object,
    transaction::Transaction,
};

pub trait WritableObjectStore {
    fn insert(&self, k: ObjectID, v: (ObjectRef, Object)) -> Option<(ObjectRef, Object)>;

    fn remove(&self, k: ObjectID) -> Option<(ObjectRef, Object)>;
}

pub fn export_to_files(
    accounts: &BTreeMap<SuiAddress, Account>,
    objects: &Vec<Object>,
    txs: &Vec<Transaction>,
    working_directory: PathBuf,
) {
    let start_time: std::time::Instant = std::time::Instant::now();

    let accounts_path = working_directory.join("accounts.dat");
    let objects_path = working_directory.join("objects.dat");
    let txs_path = working_directory.join("txs.dat");

    let accounts_s = bincode::serialize(accounts).unwrap();
    let objects_s = bincode::serialize(objects).unwrap();
    let txs_s = bincode::serialize(txs).unwrap();

    fs::write(accounts_path, accounts_s).expect("Failed to write accounts");
    fs::write(objects_path, objects_s).expect("Failed to write objects");
    fs::write(txs_path, txs_s).expect("Failed to write txs");
    let elapsed = start_time.elapsed().as_millis() as f64;
    println!("Export took {} ms", elapsed,);
}

pub fn import_from_files(
    working_directory: PathBuf,
) -> (BTreeMap<SuiAddress, Account>, Vec<Object>, Vec<Transaction>) {
    let start_time: std::time::Instant = std::time::Instant::now();

    let accounts_file = BufReader::new(
        fs::File::open(working_directory.join("accounts.dat")).expect("Failed to open accounts"),
    );
    let objects_file = BufReader::new(
        fs::File::open(working_directory.join("objects.dat")).expect("Failed to open objects"),
    );
    let txs_file = BufReader::new(
        fs::File::open(working_directory.join("txs.dat")).expect("Failed to open txs"),
    );

    let accounts = bincode::deserialize_from(accounts_file).unwrap();
    let objects = bincode::deserialize_from(objects_file).unwrap();
    let txs = bincode::deserialize_from(txs_file).unwrap();
    let elapsed = start_time.elapsed().as_millis() as f64;
    println!("Import took {} ms", elapsed,);
    (accounts, objects, txs)
}
