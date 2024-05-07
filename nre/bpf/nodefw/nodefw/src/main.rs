// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use aya::include_bytes_aligned;
use aya::programs::{Xdp, XdpFlags};
use aya::BpfLoader;
use aya_log::BpfLogger;
use clap::{Parser, ValueEnum};
use nodefw::fwmap::{ttl_watcher, Firewall};
use nodefw::time::get_ktime_get_ns;
use nodefw::{drainer, server};
use nodefw_common::Meta;
use std::sync::{Arc, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

#[derive(Debug, Parser)]
struct Opt {
    #[clap(short, long, default_value = "lo")]
    iface: String,
    #[clap(short, long, value_enum, default_value_t=Mode::Default)]
    mode: Mode,
    #[clap(short, long)]
    drain_file: String,
}

#[derive(ValueEnum, Clone, Debug)]
enum Mode {
    Default,
    Drv,
    Skb,
}

// we wrap XdpFlags in our own struct to decouple it from clap's derive requirements.
impl From<Mode> for XdpFlags {
    fn from(m: Mode) -> Self {
        match m {
            Mode::Default => XdpFlags::default(),
            Mode::Drv => XdpFlags::DRV_MODE,
            Mode::Skb => XdpFlags::SKB_MODE,
        }
    }
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

    let ctx = CancellationToken::new();
    let fw = Firewall::new("BLOCKLIST", &mut bpf);
    let fw_guard = Arc::new(RwLock::new(fw));
    tokio::spawn(ttl_watcher(ctx.clone(), fw_guard.clone()));

    let router = server::app(fw_guard.clone());
    let program: &mut Xdp = bpf.program_mut("nodefw").unwrap().try_into()?;
    program.load()?;
    program.attach(&opt.iface, XdpFlags::from(opt.mode))
        .context("failed to attach the XDP program with default flags - try changing XdpFlags::default() to XdpFlags::SKB_MODE")?;

    let listener = std::net::TcpListener::bind(nodefw::var!(
        "NODEFW_LISTEN_ON",
        "127.0.0.1:8080".into(),
        String
    ))
    .unwrap();
    info!("Listening for signal to terminate...");
    let sc = server::ServerConfig {
        ctx: ctx.clone(),
        listener,
        router,
    };
    loop {
        tokio::select! {
            _ = server::serve(sc) => {
                ctx.cancel();
                break;
            },
            _ = drainer::watch(ctx.clone(), &opt.drain_file) => {
                ctx.cancel();
                break;
            },
        }
    }
    info!("firewall has stopped, exiting.");
    Ok(())
}
