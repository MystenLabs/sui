use fastx_types::base_types::*;

use dialoguer::Input;
use std::path::PathBuf;

#[macro_use]
extern crate prettytable;
use prettytable::Table;
use structopt::clap::AppSettings;
use structopt::StructOpt;

/// Launch FastX
#[derive(StructOpt)]
#[structopt(setting(AppSettings::NoBinaryName))]
#[structopt(rename_all = "kebab-case")]
#[structopt(name = "", about = "FastX Cli")]
enum ClientCommands {
    /// Start FastX service
    #[structopt(name = "start")]
    StartFastX {
        /// Genesis state config file path
        #[structopt(long)]
        genesis: Option<PathBuf>,
    },

    /// Get all obj info for this (hex encoded) address
    #[structopt(name = "objects")]
    GetAllObjects {
        /// Address of the account
        #[structopt(long, parse(try_from_str = parse_public_key_bytes))]
        address: PublicKeyBytes,
    },

    /// Get obj info for this address
    #[structopt(name = "object")]
    GetObject {
        /// Object ID in 20 bytes Hex string
        #[structopt(long)]
        object_id: ObjectID,

        /// Optional version of the object
        #[structopt(long)]
        version: Option<usize>,
    },
    /// Get the certificate with the given transaction digest
    #[structopt(name = "certificate")]
    GetCertificate {
        /// Object ID in 20 bytes Hex string
        #[structopt(long, parse(try_from_str = decode_tx_digest_hex))]
        tx_digest: TransactionDigest,
    },

    /// Creates a new key pair
    #[structopt(name = "new_address")]
    CreateNewAddress {},

    /// List all addresses managed by client
    #[structopt(name = "addresses")]
    ListAllAddresses {},

    /// Transfer object
    #[structopt(name = "transfer")]
    Transfer {
        /// Sender's address
        #[structopt(long, parse(try_from_str = parse_public_key_bytes))]
        sender: PublicKeyBytes,

        /// Recipient address
        #[structopt(long, parse(try_from_str = parse_public_key_bytes))]
        recipient: PublicKeyBytes,

        /// Object to transfer, in 20 bytes Hex string
        #[structopt(long)]
        object: ObjectID,

        /// ID of the gas object for gas payment, in 20 bytes Hex string
        #[structopt(long)]
        gas: ObjectID,
    },

    /// Call Move function in module
    #[structopt(name = "call")]
    Call {
        /// Sender's address
        #[structopt(long, parse(try_from_str = parse_public_key_bytes))]
        sender: PublicKeyBytes,

        /// Object ID of the package, in 20 bytes Hex string
        #[structopt(long)]
        package: ObjectID,

        /// Name of the module
        #[structopt(long)]
        module: String,

        /// Name of the function
        #[structopt(long)]
        function: String,

        /// Type arguments
        #[structopt(long)]
        type_args: Vec<String>,

        /// Object IDs of the object arguments
        #[structopt(long)]
        object_args: Vec<ObjectID>,

        /// Pure arguments
        #[structopt(long)]
        pure_args: Vec<String>,

        /// ID of the gas object for gas payment, in 20 bytes Hex string
        #[structopt(long)]
        gas_object_id: ObjectID,
    },

    /// Publish Move package
    #[structopt(name = "publish")]
    Publish {
        /// Sender's address
        #[structopt(long, parse(try_from_str = parse_public_key_bytes))]
        sender: PublicKeyBytes,

        /// Hex encoded package bytes
        #[structopt(long)]
        package: Vec<u8>,

        /// ID of the gas object for gas payment, in 20 bytes Hex string
        #[structopt(long)]
        gas_object_id: ObjectID,
    },
}
fn parse_public_key_bytes(src: &str) -> Result<PublicKeyBytes, hex::FromHexError> {
    decode_address_hex(src)
}

fn main() {
    const FASTX_PROMPT: &str = "fastx>";

    loop {
        // The main input string
        let input_str = Input::<String>::new()
            .allow_empty(true)
            .with_prompt(FASTX_PROMPT)
            //.history_with()
            .interact_text()
            .unwrap();
        // Tokenize the string
        let tokens = input_str.split(' ').collect::<Vec<_>>();

        // Support hitting return key with no tokens
        if tokens.len() == 1 && tokens[0].is_empty() {
            continue;
        }

        // Load the tokens as commands into structs
        let cli_options = ClientCommands::from_iter_safe(tokens);

        // If cast to struct fails, print custom
        if cli_options.is_err() {
            let err_str = cli_options.err().unwrap().message;
            write_er!("{}", err_str);
            continue;
        }

        // Get a formatted output from the execution of the command
        let table = match cli_options.ok().unwrap() {
            ClientCommands::StartFastX { genesis } => todo!(),
            ClientCommands::GetAllObjects { address } => format_random_objects(),
            ClientCommands::GetObject { object_id, version } => todo!(),
            ClientCommands::GetCertificate { tx_digest } => todo!(),
            ClientCommands::CreateNewAddress {} => todo!(),
            ClientCommands::ListAllAddresses {} => format_random_addresses(),
            ClientCommands::Transfer {
                sender,
                recipient,
                object,
                gas,
            } => todo!(),
            ClientCommands::Call {
                sender,
                package,
                module,
                function,
                type_args,
                object_args,
                pure_args,
                gas_object_id,
            } => todo!(),
            ClientCommands::Publish {
                sender,
                package,
                gas_object_id,
            } => todo!(),
        };

        // Print the output
        table.printstd();
    }
}

/// Dummy function to format adresses
fn format_random_addresses() -> Table {
    let mut table = table!(["Public Key"]);
    for _ in 0..10 {
        let obj = format!("{:02X}", get_key_pair().0);
        table.add_row(row![obj]);
    }

    table
}

/// Dummy function to format addresses
fn format_random_objects() -> Table {
    let mut table = table!(["ObjectID", "Version", "Digest"]);

    for i in 0..10 {
        let obj = format!("{}", ObjectID::random());
        let seq = format!("{:?}", SequenceNumber::from(i));
        let dig = format!("{:?}", ObjectDigest::new([i as u8; 32]));

        table.add_row(row![obj, seq, dig]);
    }

    table
}

/// Macro for printing error
macro_rules! write_er(
    (@nopanic $($arg:tt)*) => ({
        use std::io::{Write, stderr};
        let _ = writeln!(&mut stderr().lock(), $($arg)*);
    });
    ($($arg:tt)*) => ({
        use std::io::{Write, stderr};
        writeln!(&mut stderr(), $($arg)*).ok();
    })
);
pub(crate) use write_er;
