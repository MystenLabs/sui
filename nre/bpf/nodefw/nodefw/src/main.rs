// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use aya::include_bytes_aligned;
use aya::programs::{Xdp, XdpFlags};
use aya::BpfLoader;
use aya_log::BpfLogger;
use clap::Parser;
use log::{debug, info, warn};
use nodefw::fwmap::{ttl_watcher, Firewall};
use nodefw::time::{get_ktime_get_ns, ttl};
use nodefw_common::{Meta, Rule};
use std::cell::RefCell;
use std::time::Duration;
use tokio::signal;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Parser)]
struct Opt {
    #[clap(short, long, default_value = "lo")]
    iface: String,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let opt = Opt::parse();
    env_logger::init();
    // Bump the memlock rlimit. This is needed for older kernels that don't use the
    // new memcg based accounting, see https://lwn.net/Articles/837122/
    let rlim = libc::rlimit {
        rlim_cur: libc::RLIM_INFINITY,
        rlim_max: libc::RLIM_INFINITY,
    };
    let ret = unsafe { libc::setrlimit(libc::RLIMIT_MEMLOCK, &rlim) };
    if ret != 0 {
        debug!("remove limit on locked memory failed, ret is: {}", ret);
    }

    let mut loader = BpfLoader::new();
    let meta = Meta {
        ktime: get_ktime_get_ns(),
    };
    // bool == require META to be declared in ebpf code
    loader.set_global("META", &meta, true);

    // This will include your eBPF object file as raw bytes at compile-time and load it at
    // runtime. This approach is recommended for most real-world use cases. If you would
    // like to specify the eBPF program at runtime rather than at compile-time, you can
    // reach for `Bpf::load_file` instead.
    #[cfg(debug_assertions)]
    let mut bpf = loader.load(include_bytes_aligned!(
        "../../target/bpfel-unknown-none/debug/nodefw"
    ))?;
    #[cfg(not(debug_assertions))]
    let mut bpf = loader.load(include_bytes_aligned!(
        "../../target/bpfel-unknown-none/release/nodefw"
    ))?;
    if let Err(e) = BpfLogger::init(&mut bpf) {
        // This can happen if you remove all log statements from your eBPF program.
        warn!("failed to initialize eBPF logger: {}", e);
    }

    // TODO replace with rpc, this is just for testing locally
    // preload static firewall rules
    let mut fw = Firewall::new("BLOCKLIST", &mut bpf);
    fw.add(
        "192.168.1.1",
        Rule {
            ttl: ttl(Duration::from_secs(5)),
            port: 2046,
        },
    )?;
    fw.add(
        "192.168.2.1",
        Rule {
            ttl: ttl(Duration::from_secs(5)),
            port: 2046,
        },
    )?;
    fw.add("::1", Rule { ttl: 0, port: 2046 })?;

    let fw_ref = RefCell::new(fw);
    let ctx = CancellationToken::new();
    let ttl_watcher_token = ctx.clone();

    let ttl_watcher_task = tokio::spawn(ttl_watcher(ttl_watcher_token.clone(), fw_ref));
    let program: &mut Xdp = bpf.program_mut("nodefw").unwrap().try_into()?;
    program.load()?;
    program.attach(&opt.iface, XdpFlags::default())
        .context("failed to attach the XDP program with default flags - try changing XdpFlags::default() to XdpFlags::SKB_MODE")?;

    info!("Waiting for Ctrl-C...");
    signal::ctrl_c().await?;
    ttl_watcher_token.cancel();
    info!("Gracefully exiting...");
    ttl_watcher_task.await?;
    info!("Watcher task has stopped, exiting.");
    Ok(())
}
