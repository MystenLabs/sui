# On-host measurement when metrics are not enough

Get on a host with Teleport (see deploy.md). Prefer measuring an outlier host and a healthy peer side by side.

## Quick host health (do this before anything deeper)

```
uptime; nproc; free -g; df -h /opt/sui
top -b -n1 -H -p $(pgrep -x sui-node) | head -40      # per-thread CPU; which tokio/rayon threads are hot?
iostat -x 1 5                                           # NVMe util%, await; >80% util or ms-level await = disk-bound
sar -n DEV 1 5                                          # per-NIC rx/tx bytes; compare against link speed (ethtool <if> | grep Speed)
ss -s; ss -ti dst :8081 | head                          # socket summary; per-connection rtt, cwnd, retrans, unacked
nstat -az | grep -Ei 'retrans|listendrop|overflow|prune|collapse'   # TCP retransmits and receive-buffer pressure
dmesg -T | tail -50                                     # NIC resets, OOM, throttling
journalctl -u sui-node --since -30min | grep -Ei 'warn|error' | sort | uniq -c | sort -rn | head
```
Hardware suspicion checklist: CPU governor and frequency (`cpupower frequency-info`), thermal throttling in `dmesg`,
NVMe SMART (`nvme smart-log`), memory errors (`edac-util`, `dmesg`), NIC errors (`ethtool -S <if> | grep -i err`).

## CPU profiles

Release builds keep frame pointers and symbol names (`Cargo.toml` release profile, `.cargo/config.toml`), so `perf`
gives usable stacks without extra debuginfo.

```
sudo perf record -F 99 -g -p $(pgrep -x sui-node) -- sleep 30
sudo perf report --no-children --sort comm,dso,symbol | head -80      # flat hot symbols
sudo perf script | stackcollapse-perf.pl | flamegraph.pl > /tmp/flame.svg   # if FlameGraph scripts are present
```
Read it as: which pipeline stage owns the hot frames (execution, consensus core, consensus handler, checkpoint
builder, RocksDB, crypto verification, serialization), and whether the shape differs between the outlier and a
healthy peer. Use `perf top -p <pid>` for a live view and `perf stat -p <pid> -- sleep 10` for IPC/context switches
(low IPC with high CPU suggests memory-bound or lock spinning). Per-thread names from `top -H` map to tokio worker,
rayon, and named threads (`consensus-*`, `execution-*`).

Other built-in hooks:
- Admin port 1337 (localhost only): `curl -X POST 'localhost:1337/logging' -d 'sui_core=debug'` changes the log
  filter live; `curl -X POST 'localhost:1337/enable-tracing?filter=sui=trace&duration=60s'` turns on sampled
  tracing temporarily. `curl localhost:1337/node-config` dumps the effective config.
- tokio-console on :6669 shows stuck/slow tasks and busy runtime workers.
- jemalloc heap dumps land in `/opt/sui/jeprof/` if memory growth is suspected.
- Local metrics: `curl -s localhost:9184/metrics` (node), `localhost:8081/metrics` (stress).

## Network capture and analysis

Consensus traffic is TCP on 8081, state sync is UDP on 8084, gRPC/anemo on 8080. Capture only what you need:
```
sudo tcpdump -i <if> -s 128 -w /tmp/cons.pcap 'tcp port 8081' -G 60 -W 1    # 60s, headers only
sudo tcpdump -i <if> -s 0 -c 200000 -w /tmp/full.pcap 'host <peer-ip>'      # one peer, full payload
```
Questions to answer, and how:
- Bandwidth in use vs NIC capacity: `sar -n DEV` or `ifstat`, compared to `ethtool <if>` link speed. Also
  `capinfos /tmp/cons.pcap` for the capture's average rate.
- Retransmits, dup acks, zero windows: `tshark -r x.pcap -q -z io,stat,10,"tcp.analysis.retransmission",
  "tcp.analysis.duplicate_ack","tcp.analysis.zero_window"`; live: `ss -ti` fields `retrans`, `cwnd`, `rtt`, plus
  `nstat` counters `TcpRetransSegs`, `TcpExtTCPRcvCollapsed`, `TcpExtPruneCalled` (receive buffer pressure).
- Window / buffer limits: `ss -tim` shows `rcv_space`, `skmem`; check `sysctl net.ipv4.tcp_rmem net.ipv4.tcp_wmem
  net.core.rmem_max` against `ansible/playbook/sui/roles/sui-node-internal/files/100-sui-node.conf`.
- Connection churn: `tshark -r x.pcap -Y 'tcp.flags.syn==1 && tcp.flags.ack==0' | wc -l` per minute; compare with
  consensus network connection metrics. Frequent SYNs to the same peer = reconnect loop.
- Per-peer RTT and loss: `tshark -r x.pcap -q -z conv,tcp` for byte counts per conversation; `ss -ti` for rtt/min_rtt.
  Cross-region peers (lax/ash/ams) have legitimately different RTTs.
- `network-throttling.yaml` in sui-operations can reproduce a suspected bandwidth constraint on chosen hosts.

## Other tools worth reaching for

- `strace -c -p <pid> -f -- sleep 10` for syscall mix (fsync storms, futex contention).
- `bpftrace` / `offcputime` (bcc) to see where threads block, when CPU profiles look idle but latency is high.
- RocksDB: `sui-tool db-tool` against a stopped or copied db; on-host `LOG` files under `/opt/sui/db/*/` for
  compaction and write-stall messages.
- Compare `/opt/sui/config/sui-node.yaml` between outlier and peer; config drift is a common "software-only" cause.
