// Minimal client that connects to /tmp/sui/sui_tx.sock and decodes tx frames:
//   - BE u32 length + bincode TransactionEffects
//   - BE u32 length + JSON Vec<SuiEvent>
//
// Run:
//   cargo run -p sui-core --example tx_tap
// Optional socket override via env SUI_TX_SOCKET or first CLI arg.

use std::env;
use std::io::Read;
use std::os::unix::net::UnixStream;

use sui_json_rpc_types::SuiEvent;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};

const DEFAULT_SOCKET: &str = "/tmp/sui/sui_tx.sock";

fn main() -> anyhow::Result<()> {
    let socket = env::var("SUI_TX_SOCKET")
        .ok()
        .or_else(|| env::args().nth(1))
        .unwrap_or_else(|| DEFAULT_SOCKET.to_string());

    println!("Connecting to {} ...", socket);
    let mut stream = UnixStream::connect(&socket)?;
    println!("Connected. Waiting for tx frames ...");

    loop {
        // Effects frame
        let mut len_bytes = [0u8; 4];
        stream.read_exact(&mut len_bytes)?;
        let n = u32::from_be_bytes(len_bytes) as usize;
        let mut eff = vec![0u8; n];
        stream.read_exact(&mut eff)?;
        let effects: TransactionEffects = match bincode::deserialize(&eff) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Failed to decode effects ({} bytes): {}", n, e);
                continue;
            }
        };

        // Events frame
        stream.read_exact(&mut len_bytes)?;
        let m = u32::from_be_bytes(len_bytes) as usize;
        let mut ev = vec![0u8; m];
        stream.read_exact(&mut ev)?;
        let events: Vec<SuiEvent> = match serde_json::from_slice(&ev) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Failed to decode events JSON ({} bytes): {}", m, e);
                continue;
            }
        };

        println!(
            "Tx {:?}: {} events",
            effects.transaction_digest(),
            events.len()
        );
        for e in &events {
            println!("  event_type={}", e.type_);
        }
    }
}
