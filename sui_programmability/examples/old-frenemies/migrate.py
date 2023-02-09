import os
import json
import subprocess
import sys
import time

def migrate():
    print("Migrating")
    data = json.load(open(sys.argv[1]))
    top_scores = data['data']['fields']['top_scores']
    num_scores = 0
    print('    entry fun migrate(new_leaderboard: &mut Leaderboard, cap: &AdminCap) {')
    for score in top_scores:
        name = score['fields']['name']['fields']['name']
        participation = score['fields']['participation']
        score = score['fields']['score']
        print('        old_leaderboard_insert(string::utf8(b"%s"), %s, %s, new_leaderboard, cap);' % (name, score, participation))
        num_scores += 1
        if num_scores > 2000:
            break
    print('    }')


def deploy(name):
    print("Publishing...")
    res = subprocess.run(["sui", "client", "publish", "--gas-budget", "10000", '--json'], capture_output=True)
    print("Done")
    print(res)
    output = res.stdout.decode("utf8")
    data = json.loads(output)

    if 'effects' not in data:
        print('missing effects')
        print(data)
        exit(1)
    if 'created' not in data['effects']:
        print('missing created')
        print(data['effects'])
        exit(1)

    for obj in data['effects']['events']:
        if 'publish' in obj:
            obj = obj['publish']
            pkg = obj['packageId']
        if 'newObject' in obj:
            obj = obj['newObject']
            typ = obj['objectType']
            if 'registry::Registry' in typ:
                registry = obj['objectId']
            elif 'leaderboard::Leaderboard' in typ:
                leaderboard = obj['objectId']

    sui_system = '0x5'
    # print env vars to set. copy this to frenemies instructions and/or run it locally
    print('export PKG=%s;\nexport SUI_PKG=0x2;\nexport LEADERBOARD=%s;\nexport REGISTRY=%s;\nexport SUI_SYSTEM=%s;\nexport NAME="%s;"' % (pkg, leaderboard, registry, sui_system, name))

    # call register function, using gas coin fom publish as input
    gas = data['effects']['gasObject']['reference']['objectId']
    res = subprocess.run(['sui', 'client', 'call', '--package', pkg, '--module', 'frenemies', '--function', 'register', '--args', name, registry, sui_system, '--gas-budget', '10000', '--json'], capture_output=True)
    print(res)
    output = res.stdout.decode("utf8")
    data = json.loads(output)

    # note the difference in the schema for publish: ([1] here vs `effects` above)!
    for obj in data[1]['events']:
        if 'newObject' in obj:
            obj = obj['newObject']
            typ = obj['objectType']
            if 'frenemies::Scorecard' in typ:
                scorecard = obj['objectId']

    print('export SCORECARD=%s' % scorecard)

def try_to_win():
    # assumes that you have followed https://www.notion.so/mystenlabs/Frenemies-Game-Instructions-1c24959ca8804abd9940b6a76c1f47fe to set up these env vars, or added the output from running deploy()
    pkg = os.environ['PKG']
    sui_pkg = '0x2'
    sui_system = '0x5'
    scorecard = os.environ['SCORECARD']
    leaderboard = os.environ['LEADERBOARD']

    epoch_length = 5 * 60
    while True:
        try:
            # update scorecard
            res = subprocess.run(['sui', 'client', 'call', '--package', pkg, '--module', 'frenemies', '--function', 'update', '--args', scorecard, sui_system, leaderboard, '--gas-budget', '100000', '--json'], capture_output=True)
            # get assignment
            res = subprocess.run(['sui', 'client', 'object', scorecard, '--json'], capture_output=True)
            output = res.stdout.decode("utf8")
            data = json.loads(output)
            assignment = data['data']['fields']['assignment']['fields']
            print(assignment)
            if assignment['goal'] == 0:
                # friend assignment--pick a gas coin and stake
                # this will use up all your gas coins pretty quickly unless you've hit the faucet a few times
                res = subprocess.run(['sui', 'client', 'gas', '--json'], capture_output=True)
                output = res.stdout.decode("utf8")
                data = json.loads(output)
                to_stake = data[0]['id']['id']
                validator = assignment['validator']
                res = subprocess.run(['sui', 'client', 'call', '--package', sui_pkg, '--module', 'sui_system', '--function', 'request_add_delegation', '--args', sui_system, to_stake, validator, '--gas-budget', '10000', '--json'], capture_output=True)
            # otherwise, do nothing during this epoch and hope it works out anyway
        except KeyboardInterrupt:
            exit()
        except:
            print('something failed')
        time.sleep(epoch_length)

migrate()
