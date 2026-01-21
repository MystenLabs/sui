# Helm Charts

This directory previously contained helm charts for deploying Sui RPC infrastructure.

The legacy `sui-graphql-rpc` and `sui-indexer` infrastructure has been removed.
For the current indexer and GraphQL infrastructure, see `sui-indexer-alt` and `sui-indexer-alt-graphql`.

### Database Provisioning
More Documentation on this coming soon! The storage amount recommendations will increase over time. The below numbers may quickly become outdated.

*Resource Recommendations*
- Storage
    - Mainnet 30TB
    - Testnet 12TB
- Self hosted
    - 16 cores / 32 vCPU
    - 128G memory
- Cloud
    - GCP Cloud SQL
        - Example machine configuration: db-perf-optimized-N-16
    - AWS RDS
        - Example instance class: db.r6g.xlarge

# How to run Helm for Deployments

- [Install Helm](https://helm.sh/docs/intro/install/) version 3+
- Clone sui repo
- Navigate to nre/helm
- Modify values.yaml files for the service you want to deploy with helm
- Ensure changes still render. Note that successful render does not mean a successful deploy but its step 1 regardless. `helm template <chart-name> --debug`
- Ensure your local kube context is pointing to your desired cluster and namespace.
- `helm install <name> <chart-name>`
- Iterate on values/templates to fit any specific needs the run `helm upgrade <name> <chart-name>`

### Other Helm actions
***review deployment history*** - `helm history <name>`

***uninstall helm provisioned resources*** - `helm uninstall <name>`

Review [Helm docs](https://helm.sh/docs/) for other actions.


