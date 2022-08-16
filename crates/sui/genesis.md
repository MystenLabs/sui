# Genesis Ceremony

This document lays out the step-by-step process for orchestrating a Sui Genesis Ceremony.

## Prerequisites 

Each validator participating in the ceremony will need the following:

- Ed25519 Public key
- Sui network address // WAN
- Narwhal_primary_to_primary network address // WAN
- Narwhal_worker_to_primary network address // LAN
- Narwhal_primary_to_worker network address // LAN
- Narwhal_worker_to_worker network address // WAN
- Narwhal_consensus_address network address // LAN

Note:
- Network addresses should be Multiaddrs in the form of `/dns/{dns name}/tcp/{port}/http` and
only the addresses marked WAN need to be publicly accessible by the wider internet.
- An Ed25519 key can be created using `sui keytool generate`

## Ceremony

1. Creation of a shared workspace

To start, you'll need to create a shared workspace where all validators will be able to share their
information. For these instructions, we'll assume that such a shared workspace is created and managed
using a git repository hosted on git hosting provider.

The MC (Master of Ceremony) will create a new git repository and initialize the directory:

```
$ git init genesis && cd genesis
$ sui genesis-ceremony 
$ git add .
$ git commit -m "init genesis"
$ git push
```

2. Contribute Validator information

Once the shared workspace has been initialized, each validator can contribute their information:

```
$ git clone <url to genesis repo> && cd genesis
$ sui genesis-ceremony add-validator \
    --name <human-readable validator name> \
    --key-file <path to key file> \
    --network-address <multiaddr> \
    --narwhal-primary-to-primary <multiaddr> \
    --narwhal-worker-to-primary <multiaddr> \
    --narwhal-primary-to-worker <multiaddr> \
    --narwhal-worker-to-worker <multiaddr> \
    --narwhal-consensus-address <multiaddr>

$ git add .
$ git commit -m "add validator <name>'s information"
$ git push # either to the shared workspace or another branch followed by a PR
```

3. Add Initial Gas Objects

Add configuration for any initial gas objects that should be created at genesis.

```
$ sui genesis-ceremony add-gas-object \
    --address <SuiAddress> \
    --object-id <ObjectId> \
    --valud <# of sui coins>
$ git add .
$ git commit -m "add gas object"
$ git push
```

4. Build Genesis

Once all validators and gas objects have been added, the MC can build the genesis object:

```
$ sui genesis-ceremony build
$ git add .
$ git commit -m "build genesis"
$ git push
```

5. Verify and Sign Genesis

Once genesis is built each validator will need to verify and sign genesis:

```
$ sui genesis-ceremony verify-and-sign \
    --key-file <path to key file>
$ git add .
$ git commit -m "sign genesis"
$ git push
```

6. Finalize Genesis

Once all validators have successfully verified and signed genesis, the MC can finalize the ceremony
and then the genesis state can be distributed:

```
$ sui genesis-ceremony finalize
```
