from json import load


class CommitteeParser:
    def __init__(self, committee_file):
        assert isinstance(committee_file, str)
        with open(committee_file, 'r') as f:
            self.committee = load(f)

    def size(self):
        return len(self.committee['authorities'])

    def to_json(self):
        return self.committee

    def size(self):
        return len(self.committee['authorities'])

    def primaries_hosts(self):
        authorities = self.committee['authorities']
        return [x['primary']['host'] for x in authorities]

    def workers_hosts(self):
        authorities = self.committee['authorities']
        return [y[1]['host'] for x in authorities for y in x['workers']]

    def all_hosts(self):
        return self.primaries_hosts() + self.workers_hosts()

    def names(self):
        names = {}
        for authority in self.committee['authorities']:
            workers = {x[1]['name'] for x in authority['workers']}
            names[authority['primary']['name']] = workers
        return names

    def workers_ports(self):
        authorities = self.committee['authorities']
        return [y[1]['port'] for x in authorities for y in x['workers']]

    def update_hosts(self, hosts):
        assert isinstance(hosts, dict)
        for authority in self.committee['authorities']:
            machine = authority['primary']
            ip, port = hosts[machine['name']]
            machine['port'] = port
            machine['host'] = ip
            for worker in authority['workers']:
                machine = worker[1]
                ip, port = hosts[machine['name']]
                machine['port'] = port
                machine['host'] = ip

    def remove_faulty(self, f):
        assert isinstance(f, int)
        n = self.size()
        self.committee['authorities'] = self.committee['authorities'][:n-f]