// Minimal client that connects to /tmp/sui/sui_cache_updates.sock and
// decodes cache update frames: LE u32 length + bcs Vec<(ObjectID, Object)>.
//
// Run:
//   cargo run -p sui-core --example cache_tap
// Optional socket override via env SUI_CACHE_SOCKET or first CLI arg.

use std::env;
use std::io::Read;
use std::os::unix::net::UnixStream;

use sui_types::base_types::ObjectID;
use sui_types::object::Object;

const DEFAULT_SOCKET: &str = "/tmp/sui/sui_cache_updates.sock";

fn main() -> anyhow::Result<()> {
    let socket = env::var("SUI_CACHE_SOCKET")
        .ok()
        .or_else(|| env::args().nth(1))
        .unwrap_or_else(|| DEFAULT_SOCKET.to_string());

    println!("Connecting to {} ...", socket);
    let mut stream = UnixStream::connect(&socket)?;
    println!("Connected. Waiting for cache update frames ...");

    loop {
        let mut len_bytes = [0u8; 4];
        stream.read_exact(&mut len_bytes)?;
        let n = u32::from_le_bytes(len_bytes) as usize;
        let mut buf = vec![0u8; n];
        stream.read_exact(&mut buf)?;

        let objs: Vec<(ObjectID, Object)> = match bcs::from_bytes(&buf) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Failed to bcs-decode payload ({} bytes): {}", n, e);
                continue;
            }
        };

        println!("Received {} updated objects", objs.len());
        for (id, o) in objs.iter() {
            println!(
                "  id={} version={} owner={:?}",
                id,
                o.version(),
                o.owner()
            );
        }
    }
}

