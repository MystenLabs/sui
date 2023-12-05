use std::{collections::BTreeMap, fs, io::BufReader, path::PathBuf, time::Duration};

use sui_single_node_benchmark::{
    benchmark_context::BenchmarkContext,
    command::{Component, WorkloadKind},
    mock_account::Account,
    workload::Workload,
};
use sui_types::{base_types::SuiAddress, object::Object, transaction::Transaction};

pub const WORKLOAD: WorkloadKind = WorkloadKind::NoMove;
pub const COMPONENT: Component = Component::PipeTxsToChannel;

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

pub async fn generate_benchmark_data(
    tx_count: u64,
    duration: Duration,
) -> (BenchmarkContext, Vec<Transaction>) {
    let workload = Workload::new(tx_count * duration.as_secs(), WORKLOAD);
    println!(
        "Setting up benchmark...{tx_count} txs per second for {} seconds",
        duration.as_secs()
    );
    let start_time = std::time::Instant::now();
    let mut ctx = BenchmarkContext::new(workload, COMPONENT, 0).await;
    let elapsed = start_time.elapsed().as_millis() as f64;
    println!(
        "Benchmark setup finished in {}ms at a rate of {} accounts/s",
        elapsed,
        1000f64 * workload.num_accounts() as f64 / elapsed
    );

    let start_time = std::time::Instant::now();
    let tx_generator = workload.create_tx_generator(&mut ctx).await;
    let transactions = ctx.generate_transactions(tx_generator).await;
    let elapsed = start_time.elapsed().as_millis() as f64;
    println!(
        "{} txs generated in {}ms at a rate of {} TPS",
        transactions.len(),
        elapsed,
        1000f64 * workload.tx_count as f64 / elapsed,
    );

    (ctx, transactions)
}

#[cfg(test)]
mod test {
    use std::{fs, time::Duration};

    use super::import_from_files;

    #[tokio::test]
    async fn export_test() {
        let tx_count = 300;
        let duration = Duration::from_secs(10);
        let working_directory = "~/test_export";

        fs::create_dir_all(&working_directory).expect(&format!(
            "Failed to create directory '{}'",
            working_directory
        ));

        let (ctx, txs) = super::generate_benchmark_data(tx_count, duration).await;
        super::export_to_files(
            ctx.get_accounts(),
            ctx.get_genesis_objects(),
            &txs,
            working_directory.into(),
        );
        let (read_accounts, read_objects, read_txs) = import_from_files(working_directory.into());
        assert_eq!(read_accounts.len(), ctx.get_accounts().len());
        assert_eq!(&read_objects, ctx.get_genesis_objects());
        assert_eq!(read_txs, txs);
    }
}
