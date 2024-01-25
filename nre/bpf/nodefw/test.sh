ip link set lo up
ip tuntap add name tun0 mode tun
ip link set tun0 up
ip addr add 192.0.2.1 peer 192.0.2.2 dev tun0
ip route add 0.0.0.0/0 via 192.0.2.2 dev tun0

iptables -t raw -A PREROUTING -j CT
iptables -t mangle -A PREROUTING
iptables -t raw -A OUTPUT -p tcp --sport 80 -j DROP

echo "[*] tcpdump"
tcpdump -ni any -B 16384 -ttt 2>/dev/null &
TCPDUMP_PID=$!
function finish_tcpdump {
    kill ${TCPDUMP_PID}
    wait
}
trap finish_tcpdump EXIT

sleep 0.3
./venv/bin/python3 send_syn.py

echo "[*] conntrack"
conntrack -L

echo "[*] iptables -t raw"
iptables -nvx -t raw -L PREROUTING
echo "[*] iptables -t mangle"
iptables -nvx -t mangle -L PREROUTING

sudo ip link add lo1 type dummy
sudo ip link set lo1 up
sudo ip addr add 192.168.1.1 dev lo1

sudo ip link add lo2 type dummy
sudo ip link set lo2 up
sudo ip addr add 192.168.2.1 dev lo2

