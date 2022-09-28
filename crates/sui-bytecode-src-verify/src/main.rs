mod lib;

use clap::Parser;

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
struct Arguments {

}

fn main() {
    println!("Sui Move bytecode source verifier\n");
}
