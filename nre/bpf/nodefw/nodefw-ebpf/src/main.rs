// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![no_std]
#![no_main]

use aya_bpf::bindings::BPF_F_NO_PREALLOC;
use aya_bpf::{
    bindings::xdp_action,
    helpers::gen::bpf_ktime_get_ns,
    macros::{map, xdp},
    maps::HashMap,
    programs::XdpContext,
};
use aya_log_ebpf::{error, info};
use core::{mem, ptr::read_volatile};
// TODO see if this is preferred over ptr_at
// use memoffset::offset_of;
use network_types::{
    eth::{EthHdr, EtherType},
    ip::{IpProto, Ipv4Hdr, Ipv6Hdr},
    tcp::TcpHdr,
    udp::UdpHdr,
};
use nodefw_common::{Meta, Rule};

#[no_mangle]
static META: Meta = Meta { ktime: 0 };

/// MAX_BLOCKLIST_SIZE is the max number of entries we'll allow in our map
const MAX_BLOCKLIST_SIZE: u32 = 1024;

// the key is an ipv4 or ipv6 octet value expressed as an array.
#[map]
static BLOCKLIST: HashMap<[u8; 16usize], Rule> =
    HashMap::with_max_entries(MAX_BLOCKLIST_SIZE, 0 | BPF_F_NO_PREALLOC);

/// block_ip inspects our blocklist against the incoming packet and makes a filter determination
fn block_ip(ctx: &XdpContext, address: [u8; 16usize], dest_port: u16) -> bool {
    unsafe {
        if let Some(rule) = BLOCKLIST.get(&address) {
            if rule.port != dest_port {
                return false;
            }
            if rule.ttl > 0 {
                return ttl_active(ctx, rule.ttl);
            }
        }
        false
    }
}

#[xdp]
pub fn nodefw(ctx: XdpContext) -> u32 {
    match try_nodefw(ctx) {
        Ok(ret) => ret,
        Err(_) => xdp_action::XDP_ABORTED,
    }
}

#[inline(always)]
fn ptr_at<T>(ctx: &XdpContext, offset: usize) -> Result<*const T, ()> {
    let start = ctx.data();
    let end = ctx.data_end();
    let len = mem::size_of::<T>();

    if start + offset + len > end {
        return Err(());
    }

    Ok((start + offset) as *const T)
}

/// get_dest_port will get the destination tcp/udp port for a given context
fn get_dest_port(ctx: &XdpContext, af: IpProto, proto: IpProto) -> Result<u16, ()> {
    let offset = match af {
        IpProto::Ipv4 => Ipv4Hdr::LEN,
        IpProto::Ipv6 => Ipv6Hdr::LEN,
        _ => {
            error!(ctx, "invalid address family!");
            return Err(());
        }
    };
    let port = match proto {
        IpProto::Tcp => {
            let tcphdr: *const TcpHdr = ptr_at(&ctx, EthHdr::LEN + offset)?;
            let port = u16::from_be(unsafe { (*tcphdr).dest });
            port
        }
        IpProto::Udp => {
            let udphdr: *const UdpHdr = ptr_at(&ctx, EthHdr::LEN + offset)?;
            u16::from_be(unsafe { (*udphdr).dest })
        }
        _ => 0,
    };
    Ok(port)
}

/// get_duration is for debugging only - it fetches a meta data time when our
/// program started and converts it to seconds from nanosecs
fn get_duration() -> u64 {
    let meta: Meta;
    let ktime: u64;
    unsafe {
        meta = read_volatile(&META);
        ktime = bpf_ktime_get_ns();
    }
    (ktime - meta.ktime) / 1_000_000_000
}

/// ttl_active checks the ktime relative to a precomputed future ktime
/// in our rule map. we detect expired rules via this mechanism
fn ttl_active(ctx: &XdpContext, ttl: u64) -> bool {
    let ktime: u64 = unsafe { bpf_ktime_get_ns() };
    // use i64 to detect negatives, which means we expired a ttl
    let remaining: i64 = ttl as i64 - ktime as i64;
    // info!(ctx, "{} {} {}", ttl, ktime, remaining);
    remaining >= 0
}

/// eval_ip for ipv4 packets
fn eval_ip(ctx: XdpContext) -> Result<u32, ()> {
    let ipv4hdr: *const Ipv4Hdr = ptr_at(&ctx, EthHdr::LEN)?;
    let mut source_addr: [u8; 16usize] = [0; 16];
    source_addr[12..].copy_from_slice(unsafe { &(*ipv4hdr).src_addr.to_le_bytes() });
    let src_addr: u32 = u32::from_be_bytes(source_addr[12..].try_into().unwrap());
    let dest_port = get_dest_port(&ctx, IpProto::Ipv4, unsafe { (*ipv4hdr).proto })?;
    if block_ip(&ctx, source_addr, dest_port) {
        info!(&ctx, "block {:i}:{}", src_addr, dest_port);
        return Ok(xdp_action::XDP_DROP);
    }
    Ok(xdp_action::XDP_PASS)
}

/// eval_ipv6 for ipv6 packets
fn eval_ipv6(ctx: XdpContext) -> Result<u32, ()> {
    let ipv6hdr: *const Ipv6Hdr = ptr_at(&ctx, EthHdr::LEN)?;
    let source_addr = unsafe { (*ipv6hdr).src_addr.in6_u.u6_addr8 };
    let dest_port = get_dest_port(&ctx, IpProto::Ipv6, unsafe { (*ipv6hdr).next_hdr })?;
    if block_ip(&ctx, source_addr, dest_port) {
        info!(&ctx, "block {:i}:{}", source_addr, dest_port);
        return Ok(xdp_action::XDP_DROP);
    }
    Ok(xdp_action::XDP_PASS)
}

/// try_nodefw is our main entry for this program
fn try_nodefw(ctx: XdpContext) -> Result<u32, ()> {
    let ethhdr: *const EthHdr = ptr_at(&ctx, 0)?;
    return match unsafe { (*ethhdr).ether_type } {
        EtherType::Ipv4 => eval_ip(ctx),
        EtherType::Ipv6 => eval_ipv6(ctx),
        _ => return Ok(xdp_action::XDP_PASS),
    };
}

/// panic for required aya bits
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}
