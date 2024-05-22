# Helm Charts
This directory contains helm charts to deploy Sui RPC2.0 infra. These charts are intended to be starting points for RPC2.0 providers. Not everything here will necessarily work out of the box to fit each providers need. Some of this infra has some scope/setup needed outside of these charts. It's encouraged to clone these files and tweak to your infra needs.

(Currently) Out of Scope of these Helm charts:
- ingress setup
- sui-node config file
- db-url/pass secret creation
- CloudSQL Database provisioning

# Note
### DB_URL Secret for indexers/graphql
For RPC2.0 services it's recommended that you create a K8's secret that contains the DB_URL. The indexer-reader/writer and graphql charts will assume a secret exists. Ensure the secret name matches that what is in the env section in the values.yaml file. 

Example:
```
kubectl create secret generic db-secret \
  --from-literal=db-url='postgres://username:password@host:port/dbname'
  ```

### Database Provisioning
More Documentation on this coming soon! The storage amount recommendations will increase over time. The below numbers may quickly become outdated.

*Resource Reccomendations*
- Storage
    - Mainnet 30TB (24TB currently used - May 2024)
    - Testnet 12TB (9TB currently used - May 2024)
- Self hosted
    - 16 cores / 32 vCPU
    - 128G memory
- Cloud
    - GCP Cloud SQL
        - Example machine configuration: db-perf-optimized-N-16
    - AWS RDS
        - Example instance class: db.r6g.xlarge


# GraphQL
### Containers
*graphql* - container running sui-graphql rpc endpoint. Requires a Database updated by the indexer-writer.

### Resource Recommendations

*CpuRequest:* 24

*MemRequest:* 96G

# Indexer-Reader
### Containers
*indexer-reader* - sui-indexer-reader jsonrpc endpoint. This offers backwards compatibility with current sui-node json rpc. This deployment is only needed as we transition to the graphql service. This is not needed if you do not want to support the jsonrpc.

### Resource Recommendations
*Cpu:* 4 cores / 8 vCPU

*Memory:* 32Gi

# Indexer-Writer
### Containers
*sui-indexer-writer* - Indexer writer syncs and indexes sui checkpoint data into a Postgres database. Checkpoint data can be pulled from sui-node or a cloud provider hosted bucket. More info and available buckets can be found in sui [doc site](https://docs.sui.io/guides/developer/advanced/custom-indexer#remote-reader).

### Resource Recommendations
*Cpu:* 16 cores / 32 vCPU

*Memory:* 128Gi

# Sui-node
This is a StatefulSet which runs the sui-node binary. The intended purpose is to run as an RPC Fullnode. For Sui RPC2.0 purposes, these nodes can serve checkpoint data or serve as a fallback for queries. 

### Files
You need to place a valid sui-node.yaml configuration file for your fullnode into the helm/sui-node/files directory. Documentation on full node configuration can be found in the sui [doc site](https://docs.sui.io/guides/operator/sui-full-node#configure-a-full-node).

### Containers

*Init Containers*


Genesis-Download - This container downloads the appropriate genesis.blob and saves it to /opt/sui/config.


Snapshot-Download - This container checks if there is a db directory present, if not it will download a snapshot. You will need to adjust the network and epoch to download in the values.yaml file in the snapshotConfig.command blocks. For more info on formal vs db snapshot restores see our sui [doc site](https://docs.sui.io/guides/operator/snapshots).


Sui-Node - Runs the sui-node software.

### Resource Recommendations

*Cpu:* 16 cores / 32 vCPU

*Memory:* 128Gi

*PVCSize*: 4000Gi

*StorageClass:* pd-ssd - A StorageClass that allows VolumeExpansion is recommended. More info on k8s storage classes [here](https://kubernetes.io/docs/concepts/storage/storage-classes/).


# How to run Helm for Deployments

- [Install Helm](https://helm.sh/docs/intro/install/) version 3+
- Clone sui repo
- Navigate to nre/helm
- Modify values.yaml files for the service you want to deploy with helm
- Ensure changes still render. Note that successful render does not mean a successful deploy but its step 1 regardless. `helm template <chart-name> --debug`
- Ensure your local kube context is pointing to your desired cluster and namespace.
- `helm install <name> <chart-name>` specific example `helm install my-indexer-reader indexer-reader`
- Iterate on values/templates to fit any specific needs the run `helm upgrade <name> <chart-name>`

### Other Helm actions
**review deployment history** - `helm history <name>`

**uninstall helm provisioned resources** - `helm uninstall <name>`

Review [Helm docs](https://helm.sh/docs/) for other actions.


