# Contributing

If you decide to contribute to this project, please choose the scope of your contribution (e.g.,
implement formatting for structs) and file an issue in the Sui
[repository](https://github.com/MystenLabs/sui) describing the work you plan to do, and wait for a
response from a core team member so that we can avoid duplication of efforts.

Please make sure that the code you add is well documented and that you add relevant tests - please
use existing code as guidance.

## Build and Test

To run build and test, use these commands:

```shell
pnpm build
pnpm test
```

## Updating Snapshots

To update snapshots in tests, use `UB=1` (update baseline) env variable:

```shell
UB=1 pnpm test
```

## Changeset

Make sure to run `pnpm changeset` with appropriate version for the changes made. Commit the change,
so that it is picked up by CI.
