mod lib;

use clap::Parser;

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
struct Arguments {
    /// URL to a Sui full node, or 'devnet'
    pub network: String
}

fn main() {
    println!("Sui Move bytecode source verifier\n");
}
