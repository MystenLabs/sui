from re import findall, search
from statistics import mean, stdev
from multiprocessing import Pool
from datetime import datetime


class ParseError(Exception):
    pass


class LogParser:
    def __init__(self, clients, primaries, workers, faults=0):
        assert isinstance(clients, list) and len(clients) > 0
        assert isinstance(primaries, list) and len(primaries) > 0
        assert isinstance(workers, list) and len(workers) > 0

        special_data = {}
        summary_node_data = {}
        for num,(c,w) in enumerate(zip(clients, workers)):
            sub_data = self._parse_single_client_worker(num,c,w)
            special_data.update(sub_data)
            # A structure to hold counts and times for each node
            # Structure [count of batches, [counts at each stage], [duration at each stage] ]
            summary_node_data[num] = [0, [0, 0, 0], [0, 0, 0]]


        self.faults = faults

        # Parse all logs.
        try:
            tmp = self._parse_clients(clients)
            txs_size, tx_rate, start, special_send, misses, per_client = tmp
        except ParseError as e:
            print(f'ERROR: {e}')
            import sys
            sys.exit(1)

        if misses > 0:
            print(f'WARN: clients missed their target rate {misses:,} times')

        sizes, special_txs = self._parse_workers(workers)
        tmp = self._parse_primaries(primaries)
        committee_size, total_workers, max_batch_size, make_times, \
            cert_times, commit_times = tmp

        # Compute config data.
        total_bytes = sum(sizes.values())
        self.config = (
            f' Committee size: {committee_size} nodes\n'
            f' Number of workers: {total_workers} worker(s) per node\n'
            f' Faults: {self.faults} nodes\n'
            f' Transaction size: {txs_size:,} B\n'
            f' Max batch size: {max_batch_size:,} txs\n'
            f' Transaction rate: {tx_rate:,} tx/s\n'
        )

        # Compute dag results.
        dag_time = max(cert_times.values()) - start
        batches = set(cert_times.keys())
        cert_bytes = sum(v for k, v in sizes.items() if k in batches)
        cert_bps = cert_bytes / dag_time * 1000
        cert_tps = cert_bps / txs_size
        block_lat = [cert_times[k] - v for k,
                     v in make_times.items() if k in batches]
        block_lat_ave = mean(block_lat) if block_lat else 0


        client_lat_dag = []
        client_lat_con = []
        for (num, _), (header_id, time) in special_data.items():
            summary_node_data[num][0] += 1
            # Batch in header stage
            if header_id in make_times:
                time += [ make_times[header_id] ]
                summary_node_data[num][1][0] += 1
                summary_node_data[num][2][0] += make_times[header_id] - time[0]
            else:
                continue
            # Batch in Certificate stage
            if header_id in cert_times:
                time += [ cert_times[header_id] ]
                client_lat_dag += [ cert_times[header_id] - time[0] ]
                summary_node_data[num][1][1] += 1
                summary_node_data[num][2][1] += cert_times[header_id] - time[0]
            else:
                continue
            # Batch in commit stage
            if header_id in commit_times:
                time += [ commit_times[header_id] ]
                client_lat_con += [ commit_times[header_id] - time[0] ]
                summary_node_data[num][1][2] += 1
                summary_node_data[num][2][2] += commit_times[header_id] - time[0]
            else:
                continue

        client_lat_ave = mean(client_lat_dag) if client_lat_dag else 0

        self.dag_results = (
            ' Dag Results:\n'
            f' + Total certified bytes: {round(cert_bytes):,} B\n'
            f' + Execution time: {max(round(dag_time), 0):,} ms\n'
            f' + Estimated BPS: {round(cert_bps):,} B/s\n'
            f' + Estimated TPS: {round(cert_tps):,} txs/s\n'
            f' + Block Latency: {round(block_lat_ave):,} ms\n'
            f' + Client Latency: {round(client_lat_ave):,} ms\n'
        )

        # Compute consensus results.
        consensus_time = max(commit_times.values()) - start
        batches = set(commit_times.keys())
        commit_bytes = sum(v for k, v in sizes.items() if k in batches)
        commit_bps = commit_bytes / consensus_time * 1000
        commit_tps = commit_bps / txs_size
        block_lat = [commit_times[k] - v for k,
                     v in make_times.items() if k in batches]
        block_lat_ave = mean(block_lat) if block_lat else 0
        block_lat.sort()

        client_lat_ave = mean(client_lat_con) if client_lat_dag else 0



        self.consensus_results = (
            ' Consensus Results:\n'
            f' + Total committed bytes: {round(commit_bytes):,} B\n'
            f' + Execution time: {max(round(consensus_time), 0):,} ms\n'
            f' + Estimated BPS: {round(commit_bps):,} B/s\n'
            f' + Estimated TPS: {round(commit_tps):,} txs/s\n'
            f' + Block Latency: {round(block_lat_ave):,} ms\n'
            f' + Client Latency: {round(client_lat_ave):,} ms\n'
        )

        # Set results.
        self.results = (
            '\n'
            '-----------------------------------------\n'
            ' RESULTS:\n'
            '-----------------------------------------\n'
            f'{self.config}'
            '\n'
            f'{self.dag_results}'
            '\n'
            f'{self.consensus_results}'
            '-----------------------------------------\n'
            'DETAILS:\n'
            '-----------------------------------------\n'
        )

        self.results += "Node\tHeader    \tCert    \tCommit\n"
        for num in summary_node_data:
            [count, [cA, cB, cC], [tA, tB, tC]] = summary_node_data[num]
            if count == 0:
                continue # In case this machine runs only a primar (no workers)

            timeA = int(tA/cA) if cA else 0
            timeB = int(tB/cB) if cB else 0
            timeC = int(tC/cC) if cC else 0

            self.results += f'{num}\t{timeA}ms ({cA/count: .0%})\t{timeB}ms ({cB/count: .0%})\t{timeC}ms ({cC/count:.0%})\n'

    def _to_posix(self, string):
        x = datetime.fromisoformat(string.replace('Z', '+00:00'))
        return datetime.timestamp(x) * 1000

    def _parse_clients(self, clients):
        """ Parse the clients logs. """
        assert isinstance(clients, list) and len(clients) > 0
        tx_per_client_times = []

        # Get transactions size.
        client = clients[0]
        tmp = search(r'(Transactions size) ([0-9]*)', client)
        txs_size = int(tmp.group(2))

        # Get tx rate.
        total_txs_rate = 0
        for client in clients:
            tmp = search(r'(Transactions rate) ([0-9]*)', client)
            total_txs_rate += int(tmp.group(2))

        # Compute start time.
        start_times = []
        for client in clients:
            tmp = search(r'(Start sending at) ([0-9]*)', client)
            start_times += [int(tmp.group(2))]
        start_time = mean(start_times)

        # Count special txs.
        tmp = []
        for x in clients:
            tmp_client = findall(r'([-:.T0123456789]*Z) .* special transaction', x)
            tmp += tmp_client
            tx_per_client_times += [(x, tmp_client)]
        send_special = [self._to_posix(x) for x in tmp]

        # Count the number of times the client missed its deadline.
        misses = sum(len(findall(r'rate too high', x)) for x in clients)

        return txs_size, total_txs_rate, start_time, send_special, misses, tx_per_client_times

    def _parse_primaries(self, primaries):
        """ Parse the primaries logs. """
        assert isinstance(primaries, list) and len(primaries) > 0
        primary = primaries[0]

        # Get committee size.
        tmp = search(r'Committee size: ([0-9]*)', primary)
        committee_size = int(tmp.group(1))

        # Get number of workers per primary.
        tmp = search(r'Number of workers per node: ([0-9]*)', primary)
        total_workers = int(tmp.group(1))

        # Get batch size.
        tmp = search(r'Txs batch size set to ([0-9]*)', primary)
        max_batch_size = int(tmp.group(1))

        # Compute certified transactions and end time.
        # p = Pool()
        # results = p.map(self._parse_single_primary, primaries)
        # p.close()
        results = [self._parse_single_primary(x) for x in primaries]
        make_times, cert_times, commit_times = {}, {}, {}

        # Check -- is the update below correct, namely the keys of
        # all disctionaries are disjoint?
        for make, cert, commit in results:
            for k in make:
                assert k not in make_times
            for k in cert:
                assert k not in cert_times
            for k in commit:
                assert k not in commit_times

            make_times.update(make)
            cert_times.update(cert)
            commit_times.update(commit)

        return committee_size, total_workers, max_batch_size, make_times, \
            cert_times, commit_times

    def _parse_single_primary(self, primary):

        def template_to_idx_time(template, data):
            # Pattern match to extract id, time.
            vals = findall(template, data)
            dict_vals = {}
            for (idx, time) in vals:
                # We need to keep the EARLIEST value
                if idx not in dict_vals:
                    dict_vals[idx] = int(time)
                else:
                    assert dict_vals[idx] <= int(time)
            return dict_vals

        # Parse make logs
        make = template_to_idx_time(
            r'Making header with txs digest ([^ ]+) at make time (\d+)', primary
        )

        # Parse dag certify logs.
        cert = template_to_idx_time(
            r"Received our own certified digest ([^ ]+) at cert time (\d+)", primary
        )

        # Parse commit logs.
        commit = template_to_idx_time(
            r"Commit digest ([^ ]+) at commit time (\d+)", primary
        )

        # Filter out all commits/certs that are not for blocks this primary made:
        for k in list(cert.keys()):
            if k not in make:
                del cert[k]

        for k in list(commit.keys()):
            if k not in make:
                del commit[k]


        return make, cert, commit

    def _parse_workers(self, workers):
        """ Parse workers logs. """
        # p = Pool()
        # results = p.map(self._parse_single_worker, workers)
        # p.close()
        results = [self._parse_single_worker(x) for x in workers]
        sizes, special = zip(*results)
        batch_sizes = {k: v for x in sizes for k, v in x.items()}
        special_txs = {k: v for x in special for k, v in x.items()}
        return batch_sizes, special_txs

    def _parse_single_worker(self, worker):
        vals = findall(
            r'Received a tx digest ([^ ]+) computed from (\d+) bytes of txs and with (\d+)',
            worker
        )

        sizes, specials = {}, {}
        for (idx, size, special_tag) in vals:
            sizes[idx] = int(size)
            specials[int(special_tag)] = idx

        return sizes, specials

    def _parse_single_client(self, client):
        """ Parse the clients logs. """
        tx_per_client_times = []

        # Get transactions size.
        tmp = search(r'(Transactions size) ([0-9]*)', client)
        txs_size = int(tmp.group(2))

        # Get tx rate.
        tmp = search(r'(Transactions rate) ([0-9]*)', client)
        total_txs_rate = int(tmp.group(2))

        # Compute start time.
        tmp = search(r'(Start sending at) ([0-9]*)', client)
        start_times = [int(tmp.group(2))]
        start_time = mean(start_times)

        # Count special txs.
        tmp_client = findall(r'([-:.T0123456789]*Z) .* special transaction ([0-9]+)', client)
        send_special = dict([(int(idx), self._to_posix(time)) for (time, idx) in tmp_client])

        # Count the number of times the client missed its deadline.
        misses = len(findall(r'rate too high', client))

        return txs_size, total_txs_rate, start_time, send_special, misses

    def _parse_single_client_worker(self, num, client, worker):
        txs_size, total_txs_rate, start_time, send_special, misses = self._parse_single_client(client)
        sizes, specials = self._parse_single_worker(worker)

        data = {}
        for tx_id in send_special:
            if tx_id in specials:
                data[(num, tx_id)] = (specials[tx_id], [ send_special[tx_id] ])
            # else:
            #    data[(num, tx_id)] = (None, [ send_special[tx_id] ])

        return data
