// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Maintains the checked-in, offline mainnet replay corpus.

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use sui_data_store::{EpochStore, Node, SetupStore, TransactionStore, stores::FileSystemStore};
use sui_replay_2::{
    ReplayConfigExperimental, ReplayConfigStableInternal, StoreMode, handle_replay_config,
};
use sui_types::{
    base_types::ObjectID,
    coin_reservation::ParsedDigest,
    effects::{
        InputConsensusObject, TransactionEffects, TransactionEffectsAPI, UnchangedConsensusKind,
    },
    transaction::{
        CallArg, Command, ObjectArg, Reservation, SharedObjectMutability, TransactionData,
        TransactionDataAPI, TransactionKind, WithdrawFrom, WithdrawalTypeArg,
        is_gas_paid_from_address_balance,
    },
};

const TABLE_BEGIN: &str = "<!-- offline-replay-corpus:begin -->";
const TABLE_END: &str = "<!-- offline-replay-corpus:end -->";

#[derive(Parser, Debug)]
#[command(about = "Maintain the checked-in sui-replay-2 offline replay corpus")]
struct Args {
    /// Root containing digests.txt, README.md, and the .sui_data_store cache.
    #[arg(long, global = true, default_value_os_t = default_fixture_root())]
    fixture_root: PathBuf,

    #[command(subcommand)]
    command: CorpusCommand,
}

#[derive(Subcommand, Debug)]
enum CorpusCommand {
    /// Capture mainnet transactions, append them to digests.txt, and regenerate the table.
    Add { digests: Vec<String> },
    /// Remove transactions from digests.txt and regenerate the table. Cached data is retained.
    Remove { digests: Vec<String> },
    /// Capture every digest in digests.txt that is absent from the fixture cache, then regenerate
    /// the complete README table.
    Sync,
    /// Fail unless the README table exactly matches digests.txt and the local fixture cache.
    Check,
}

fn default_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/offline-replay")
}

fn main() -> Result<()> {
    let Args {
        fixture_root,
        command,
    } = Args::parse();
    let fixture_root = fixture_root.canonicalize().with_context(|| {
        format!(
            "failed to resolve fixture root {}; pass --fixture-root to override it",
            fixture_root.display()
        )
    })?;

    // FileSystemStore resolves its root from this variable. Set it before creating the Tokio
    // runtime so no worker can observe a different value.
    unsafe {
        std::env::set_var("SUI_DATA_STORE", &fixture_root);
    }

    tokio::runtime::Runtime::new()?.block_on(run(command, &fixture_root))
}

async fn run(command: CorpusCommand, fixture_root: &Path) -> Result<()> {
    match command {
        CorpusCommand::Add { digests } => {
            validate_digest_arguments(&digests)?;
            capture_digests(&digests).await?;
            append_digests(fixture_root, &digests)?;
            sync(fixture_root)
        }
        CorpusCommand::Remove { digests } => {
            validate_digest_arguments(&digests)?;
            remove_digests(fixture_root, &digests)?;
            sync(fixture_root)
        }
        CorpusCommand::Sync => {
            capture_digests(&uncaptured_digests(fixture_root)?).await?;
            sync(fixture_root)
        }
        CorpusCommand::Check => check(fixture_root),
    }
}

fn uncaptured_digests(fixture_root: &Path) -> Result<Vec<String>> {
    let store = FileSystemStore::new(Node::Mainnet)?;
    store.setup(None)?;
    let mut uncaptured = Vec::new();
    for digest in read_digests(fixture_root)? {
        if store.transaction_data_and_effects(&digest)?.is_none() {
            uncaptured.push(digest);
        }
    }
    Ok(uncaptured)
}

async fn capture_digests(digests: &[String]) -> Result<()> {
    for digest in digests {
        println!("Capturing {digest} from mainnet...");
        let artifact_root = temporary_artifact_root()?;
        let result = handle_replay_config(
            &ReplayConfigStableInternal {
                digest: Some(digest.clone()),
                digests_path: None,
                terminate_early: true,
                trace: false,
                output_dir: Some(artifact_root.clone()),
                show_effects: false,
                overwrite: false,
                skip_artifacts: true,
            },
            &ReplayConfigExperimental {
                node: Node::Mainnet,
                verbose: false,
                store_mode: StoreMode::FsThenGql,
                track_time: false,
                cache_executor: false,
            },
            env!("CARGO_PKG_VERSION"),
        )
        .await;
        fs::remove_dir_all(&artifact_root).with_context(|| {
            format!(
                "failed to remove temporary replay artifacts at {}",
                artifact_root.display()
            )
        })?;
        result?;
    }
    Ok(())
}

fn temporary_artifact_root() -> Result<PathBuf> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after the Unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "sui-replay-2-offline-corpus-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&path).with_context(|| format!("failed to create {}", path.display()))?;
    Ok(path)
}

pub(crate) fn sync(fixture_root: &Path) -> Result<()> {
    let table = render_table(fixture_root)?;
    let readme_path = fixture_root.join("README.md");
    let readme = fs::read_to_string(&readme_path)
        .with_context(|| format!("failed to read {}", readme_path.display()))?;
    let updated = replace_table(&readme, &table)?;
    write_file(&readme_path, &updated)?;
    println!("Regenerated {}", readme_path.display());
    Ok(())
}

pub(crate) fn check(fixture_root: &Path) -> Result<()> {
    let table = render_table(fixture_root)?;
    let readme_path = fixture_root.join("README.md");
    let readme = fs::read_to_string(&readme_path)
        .with_context(|| format!("failed to read {}", readme_path.display()))?;
    let committed = table_between_markers(&readme)?;
    if committed != table {
        bail!(
            "{} is out of sync with digests.txt or the local fixture cache; run `cargo run -p \
             sui-replay-2 --example offline_replay_corpus -- sync`",
            readme_path.display()
        );
    }
    println!("{} is in sync", readme_path.display());
    Ok(())
}

fn render_table(fixture_root: &Path) -> Result<String> {
    let digests = read_digests(fixture_root)?;
    let store = FileSystemStore::new(Node::Mainnet)?;
    store.setup(None)?;

    let mut rows = Vec::new();
    for digest in digests {
        let info = store
            .transaction_data_and_effects(&digest)?
            .ok_or_else(|| {
                anyhow!(
                    "transaction {digest} is absent from the local fixture cache; run `cargo run \
                     -p sui-replay-2 --example offline_replay_corpus -- sync` to capture it"
                )
            })?;
        let epoch = info.effects.executed_epoch();
        let epoch_info = store.epoch_info(epoch)?.ok_or_else(|| {
            anyhow!("epoch {epoch} for transaction {digest} is absent from cache")
        })?;
        let row = markdown_table_row(
            &digest,
            &info.data,
            &info.effects,
            info.checkpoint,
            epoch_info.protocol_version,
        );
        rows.push((epoch, info.checkpoint, digest, row));
    }
    // digests.txt is append-only; the table reads better in chain order.
    rows.sort();

    let mut table = String::from(
        "| Epoch | Protocol | Checkpoint | Effects | Transaction shape | Digest |\n\
         | ---: | ---: | ---: | --- | --- | --- |\n",
    );
    for (_, _, _, row) in rows {
        table.push_str(&row);
        table.push('\n');
    }
    Ok(table)
}

fn markdown_table_row(
    digest: &str,
    transaction_data: &TransactionData,
    effects: &TransactionEffects,
    checkpoint: u64,
    protocol_version: u64,
) -> String {
    format!(
        "| {} | {} | {} | {} | {} | [{}](https://devxplorer.io/?search={digest}&network=mainnet) |",
        format_number(effects.executed_epoch()),
        format_number(protocol_version),
        format_number(checkpoint),
        effect_shape(transaction_data, effects),
        transaction_shape(transaction_data),
        digest,
    )
}

fn format_number(value: u64) -> String {
    let digits = value.to_string();
    let mut formatted = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index != 0 && (digits.len() - index).is_multiple_of(3) {
            formatted.push(',');
        }
        formatted.push(digit);
    }
    formatted
}

fn effect_shape(transaction_data: &TransactionData, effects: &TransactionEffects) -> String {
    let status = if effects.status().is_ok() {
        "success"
    } else {
        "failure"
    };
    let implicit_load_count = implicit_consensus_load_count(transaction_data, effects);
    if implicit_load_count == 0 {
        status.to_owned()
    } else {
        format!("{status}; {implicit_load_count} effects-only consensus load(s)")
    }
}

fn implicit_consensus_load_count(
    transaction_data: &TransactionData,
    effects: &TransactionEffects,
) -> usize {
    let declared_shared_ids = match transaction_data.kind() {
        TransactionKind::ProgrammableTransaction(ptb) => ptb
            .inputs
            .iter()
            .filter_map(|input| match input {
                CallArg::Object(ObjectArg::SharedObject { id, .. }) => Some(*id),
                CallArg::Pure(_)
                | CallArg::Object(ObjectArg::ImmOrOwnedObject(_))
                | CallArg::Object(ObjectArg::Receiving(_))
                | CallArg::FundsWithdrawal(_) => None,
            })
            .collect::<BTreeSet<_>>(),
        _ => BTreeSet::new(),
    };
    let effect_ids = effects
        .accessed_consensus_objects()
        .into_iter()
        .filter_map(|input| match input {
            InputConsensusObject::Mutate(object_ref)
            | InputConsensusObject::ReadOnly(object_ref) => Some(object_ref.0),
            InputConsensusObject::MutateConsensusStreamEnded(..)
            | InputConsensusObject::ReadConsensusStreamEnded(..)
            | InputConsensusObject::Cancelled(..) => None,
        })
        .chain(
            effects
                .unchanged_consensus_objects()
                .into_iter()
                .filter_map(|(id, kind)| {
                    matches!(kind, UnchangedConsensusKind::ReadOnlyRoot(_)).then_some(id)
                }),
        )
        .collect::<BTreeSet<ObjectID>>();
    effect_ids.difference(&declared_shared_ids).count()
}

fn transaction_shape(transaction_data: &TransactionData) -> String {
    match transaction_data.kind() {
        TransactionKind::ProgrammableTransaction(ptb) => {
            let commands = if ptb.commands.is_empty() {
                "no commands".to_owned()
            } else {
                format!("`{}`", summarize_commands(&ptb.commands))
            };
            let inputs = summarize_inputs(&ptb.inputs);
            format!(
                "{commands}; inputs: {inputs}; gas: {}",
                gas_shape(transaction_data)
            )
        }
        TransactionKind::ChangeEpoch(_) => "`ChangeEpoch`".to_owned(),
        TransactionKind::Genesis(_) => "`Genesis`".to_owned(),
        TransactionKind::ConsensusCommitPrologue(_) => "`ConsensusCommitPrologue`".to_owned(),
        TransactionKind::AuthenticatorStateUpdate(_) => "`AuthenticatorStateUpdate`".to_owned(),
        TransactionKind::EndOfEpochTransaction(_) => "`EndOfEpochTransaction`".to_owned(),
        TransactionKind::RandomnessStateUpdate(_) => "`RandomnessStateUpdate`".to_owned(),
        TransactionKind::ConsensusCommitPrologueV2(_) => "`ConsensusCommitPrologueV2`".to_owned(),
        TransactionKind::ConsensusCommitPrologueV3(_) => "`ConsensusCommitPrologueV3`".to_owned(),
        TransactionKind::ConsensusCommitPrologueV4(_) => "`ConsensusCommitPrologueV4`".to_owned(),
        TransactionKind::ProgrammableSystemTransaction(_) => {
            "`ProgrammableSystemTransaction`".to_owned()
        }
    }
}

fn gas_shape(transaction_data: &TransactionData) -> String {
    let gas_data = transaction_data.gas_data();
    if is_gas_paid_from_address_balance(gas_data, transaction_data.kind()) {
        return "address balance".to_owned();
    }
    if gas_data.is_unmetered() {
        return "unmetered".to_owned();
    }

    let mut real_coin_count = 0;
    let mut reservations = Vec::new();
    for (_id, _version, digest) in &gas_data.payment {
        match ParsedDigest::try_from(*digest) {
            Ok(reservation) => reservations.push(format!(
                "address-balance reservation (owner {}, amount {}, epoch {})",
                gas_data.owner,
                format_number(reservation.reservation_amount()),
                format_number(reservation.epoch_id()),
            )),
            Err(_) => real_coin_count += 1,
        }
    }

    let mut sources = Vec::new();
    if real_coin_count != 0 {
        sources.push(match real_coin_count {
            1 => "coin object".to_owned(),
            count => format!("{count} coin objects"),
        });
    }
    sources.extend(reservations);
    if sources.is_empty() {
        "no gas payment".to_owned()
    } else {
        sources.join(" + ")
    }
}

fn summarize_commands(commands: &[Command]) -> String {
    let mut runs = Vec::new();
    for command in commands {
        let command = match command {
            Command::MoveCall(call) => format!("MoveCall({}::{})", call.module, call.function),
            Command::TransferObjects(_, _) => "TransferObjects".to_owned(),
            Command::SplitCoins(_, _) => "SplitCoins".to_owned(),
            Command::MergeCoins(_, _) => "MergeCoins".to_owned(),
            Command::Publish(_, _) => "Publish".to_owned(),
            Command::MakeMoveVec(_, _) => "MakeMoveVec".to_owned(),
            Command::Upgrade(_, _, _, _) => "Upgrade".to_owned(),
        };

        if let Some((previous, count)) = runs.last_mut()
            && *previous == command
        {
            *count += 1;
        } else {
            runs.push((command, 1));
        }
    }

    runs.into_iter()
        .map(|(command, count)| {
            if count == 1 {
                command
            } else {
                format!("{count}x {command}")
            }
        })
        .collect::<Vec<_>>()
        .join(" -> ")
}

fn summarize_inputs(inputs: &[CallArg]) -> String {
    let mut pure = false;
    let mut owned_or_immutable = false;
    let mut coin_reservation = false;
    let mut mutable_shared = false;
    let mut immutable_shared = false;
    let mut non_exclusive_shared = false;
    let mut receiving = false;
    let mut withdrawals = Vec::new();

    for input in inputs {
        match input {
            CallArg::Pure(_) => pure = true,
            CallArg::Object(ObjectArg::ImmOrOwnedObject((_, _, digest))) => {
                if ParsedDigest::is_coin_reservation_digest(digest) {
                    coin_reservation = true;
                } else {
                    owned_or_immutable = true;
                }
            }
            CallArg::Object(ObjectArg::Receiving(_)) => receiving = true,
            CallArg::Object(ObjectArg::SharedObject { mutability, .. }) => match mutability {
                SharedObjectMutability::Mutable => mutable_shared = true,
                SharedObjectMutability::Immutable => immutable_shared = true,
                SharedObjectMutability::NonExclusiveWrite => non_exclusive_shared = true,
            },
            CallArg::FundsWithdrawal(withdrawal) => withdrawals.push(format_withdrawal(withdrawal)),
        }
    }

    let mut kinds = Vec::new();
    if pure {
        kinds.push("pure".to_owned());
    }
    if owned_or_immutable {
        kinds.push("owned/immutable".to_owned());
    }
    if coin_reservation {
        kinds.push("coin reservation".to_owned());
    }
    if mutable_shared {
        kinds.push("mutable shared".to_owned());
    }
    if immutable_shared {
        kinds.push("immutable shared".to_owned());
    }
    if non_exclusive_shared {
        kinds.push("non-exclusive shared".to_owned());
    }
    if receiving {
        kinds.push("receiving".to_owned());
    }
    kinds.extend(withdrawals);
    if kinds.is_empty() {
        "none".to_owned()
    } else {
        kinds.join(", ")
    }
}

fn format_withdrawal(withdrawal: &sui_types::transaction::FundsWithdrawalArg) -> String {
    let type_arg = match &withdrawal.type_arg {
        WithdrawalTypeArg::Balance(type_arg) => type_arg.to_string(),
    };
    let source = match &withdrawal.withdraw_from {
        WithdrawFrom::Sender => "Sender".to_owned(),
        WithdrawFrom::Sponsor => "Sponsor".to_owned(),
        WithdrawFrom::SenderAllowance { funder, allowance } => {
            format!("SenderAllowance({funder}, {allowance})")
        }
    };
    let reservation = match &withdrawal.reservation {
        Reservation::MaxAmountU64(amount) => format!("MaxAmountU64({amount})"),
    };
    format!("FundsWithdrawal({type_arg}, {source}, {reservation})")
}

fn read_digests(fixture_root: &Path) -> Result<Vec<String>> {
    let manifest = fixture_root.join("digests.txt");
    let contents = fs::read_to_string(&manifest)
        .with_context(|| format!("failed to read {}", manifest.display()))?;
    let digests = contents
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_owned())
        .collect::<Vec<_>>();
    validate_digest_arguments(&digests)?;
    Ok(digests)
}

fn validate_digest_arguments(digests: &[String]) -> Result<()> {
    if digests.is_empty() {
        bail!("at least one transaction digest is required");
    }
    let mut seen = BTreeSet::new();
    for digest in digests {
        digest
            .parse::<sui_types::digests::TransactionDigest>()
            .with_context(|| format!("invalid transaction digest {digest}"))?;
        if !seen.insert(digest) {
            bail!("transaction digest {digest} appears more than once");
        }
    }
    Ok(())
}

fn append_digests(fixture_root: &Path, additions: &[String]) -> Result<()> {
    let mut digests = read_digests(fixture_root)?;
    let existing = digests.iter().cloned().collect::<BTreeSet<_>>();
    for digest in additions {
        if existing.contains(digest) {
            bail!("transaction digest {digest} is already in digests.txt");
        }
        digests.push(digest.clone());
    }
    write_digests(fixture_root, &digests)
}

fn remove_digests(fixture_root: &Path, removals: &[String]) -> Result<()> {
    let digests = read_digests(fixture_root)?;
    let removals = removals.iter().cloned().collect::<BTreeSet<_>>();
    for digest in &removals {
        if !digests.contains(digest) {
            bail!("transaction digest {digest} is not in digests.txt");
        }
    }
    let retained = digests
        .into_iter()
        .filter(|digest| !removals.contains(digest))
        .collect::<Vec<_>>();
    if retained.is_empty() {
        bail!("refusing to leave the offline replay corpus empty");
    }
    write_digests(fixture_root, &retained)
}

fn write_digests(fixture_root: &Path, digests: &[String]) -> Result<()> {
    write_file(
        &fixture_root.join("digests.txt"),
        &format!("{}\n", digests.join("\n")),
    )
}

fn table_between_markers(readme: &str) -> Result<String> {
    let start = readme
        .find(TABLE_BEGIN)
        .ok_or_else(|| anyhow!("README is missing {TABLE_BEGIN}"))?
        + TABLE_BEGIN.len();
    let end = readme[start..]
        .find(TABLE_END)
        .map(|index| start + index)
        .ok_or_else(|| anyhow!("README is missing {TABLE_END}"))?;
    Ok(readme[start..end]
        .strip_prefix('\n')
        .unwrap_or(&readme[start..end])
        .to_owned())
}

fn replace_table(readme: &str, table: &str) -> Result<String> {
    let start = readme
        .find(TABLE_BEGIN)
        .ok_or_else(|| anyhow!("README is missing {TABLE_BEGIN}"))?
        + TABLE_BEGIN.len();
    let end = readme[start..]
        .find(TABLE_END)
        .map(|index| start + index)
        .ok_or_else(|| anyhow!("README is missing {TABLE_END}"))?;
    Ok(format!("{}\n{}{}", &readme[..start], table, &readme[end..]))
}

fn write_file(path: &Path, contents: &str) -> Result<()> {
    fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))
}
