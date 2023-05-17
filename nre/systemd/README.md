# Run a Sui Node using Systemd

Tested using:
- Ubuntu 20.04 (linux/amd64) on bare metal
- Ubuntu 22.04 (linux/amd64) on bare metal

## Prerequisites and Setup

1. Add a `sui` user and the `/opt/sui` directories

```shell
sudo useradd sui
sudo mkdir -p /opt/sui/bin
sudo mkdir -p /opt/sui/config
sudo mkdir -p /opt/sui/db
sudo mkdir -p /opt/sui/key-pairs
sudo chown -R sui:sui /opt/sui
```

2. Install the Sui Node (sui-node) binary, two options:
    
- Pre-built binary stored in Amazon S3:
        
```shell
wget https://releases.sui.io/$SUI_SHA/sui-node
chmod +x sui-node
sudo mv sui-node /opt/sui/bin
```

- Build from source:

```shell
git clone https://github.com/MystenLabs/sui.git && cd sui
git checkout $SUI_SHA
cargo build --release --bin sui-node
mv ./target/release/sui-node /opt/sui/bin/sui-node
```

3. Copy your key-pairs into `/opt/sui/key-pairs/` 

If generated during the Genesis ceremony these will be at `SuiExternal.git/sui-testnet-wave3/genesis/key-pairs/`

Make sure when you copy them they retain `sui` user permissions. To be safe you can re-run: `sudo chown -R sui:sui /opt/sui`

4. Update the node configuration file and place it in the `/opt/sui/config/` directory.

Add the paths to your private keys to validator.yaml. If you chose to put them in `/opt/sui/key-pairs`, you can use the following example: 

```
protocol-key-pair: 
  path: /opt/sui/key-pairs/protocol.key
worker-key-pair: 
  path: /opt/sui/key-pairs/worker.key
network-key-pair: 
  path: /opt/sui/key-pairs/network.key
```

5. Place genesis.blob in `/opt/sui/config/` (should be available after the Genesis ceremony)

6. Copy the sui-node systemd service unit file 

File: [sui-node.service](./sui-node.service)

Copy the file to `/etc/systemd/system/sui-node.service`.

7. Reload systemd with this new service unit file, run:

```shell
sudo systemctl daemon-reload
```

8. Enable the new service with systemd

```shell
sudo systemctl enable sui-node.service
```

## Connectivity

You may need to explicitly open the ports outlined in [Sui for Node Operators](../sui_for_node_operators.md#connectivity) for the required Sui Node connectivity.

## Start the node

Start the Validator:

```shell
sudo systemctl start sui-node
```

Check that the node is up and running:

```shell
sudo systemctl status sui-node
```

Follow the logs with:

```shell
journalctl -u sui-node -f
```

## Updates

When an update is required to the Sui Node software the following procedure can be used. It is highly **unlikely** that you will want to restart with a clean database.

- assumes sui-node lives in `/opt/sui/bin/`
- assumes systemd service is named sui-node
- **DO NOT** delete the Sui databases

1. Stop sui-node systemd service

```
sudo systemctl stop sui-node
```

2. Fetch the new sui-node binary

```shell
wget https://releases.sui.io/${SUI_SHA}/sui-node
```

3. Update and move the new binary:

```
chmod +x sui-node
sudo mv sui-node /opt/sui/bin/
```

4. start sui-node systemd service

```
sudo systemctl start sui-node
```
