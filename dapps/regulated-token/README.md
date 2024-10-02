# Closed Loop CLI Example

This small example illustrates how to use the Closed Loop + CLI for a simple token with denylist.

# Publish

Given that the CLI is set up, an account is created and has some funds, the following command will publish the contract:

```
sh publish.sh

> Environment variables for the next step:
> PKG=0xA11CE...
> POLICY=0xBOB...
> POLICY_CAP=0xCA41...
> TREASURY_CAP=0x0D15...
>
> To apply them to current process, run: 'source .env'
```

# Commands to interact with the module

This module has denylist enabled from the start and the denylist is empty. The following commands will show how to add and remove addresses from the denylist.

## Mint and transfer to an address

```bash
# arguments: TreasuryCap, amount, recipient
sui client call --json \
    --package $PKG --module reg --function mint_and_transfer \
    --gas-budget 100000000 \
    --args $TREASURY_CAP "amount" "0x_recipient"
```

## Add addresses to the denylist

```bash
# arguments: Policy, PolicyCap, addresses (vector)
sui client call --json \
    --package $PKG --module denylist_rule --function add_records \
    --gas-budget 100000000 \
    --args $POLICY $POLICY_CAP "[<0xaddress>]"
```

Here's an example of an address to add to the list: `0x2df4fa8165dd5667f2d0c63f1bab80b81e6db9e16d161facfd77b21e66e612c0`

## Remove addresses from the denylist

```bash
# arguments: Policy, PolicyCap, addresses (vector)
sui client call --json \
    --package $PKG --module denylist_rule --function remove_records \
    --gas-budget 100000000 \
    --args $POLICY $POLICY_CAP "[<0xaddress>]"
```

# User commands

## Transfer (whole object)

```bash
# arguments: amount, recipient
sui client call --json \
    --package $PKG --module reg --function transfer \
    --gas-budget 100000000 \
    --args $POLICY "0x_token" "0x_recipient"
```

## Split and Transfer

```bash
# arguments: amount, recipient
sui client call --json \
    --package $PKG --module reg --function split_and_transfer \
    --gas-budget 100000000 \
    --args $POLICY "0x_token" "amount" "0x_recipient"
```

## Spend

```bash
# arguments: token, amount
sui client call --json \
    --package $PKG --module reg --function spend \
    --gas-budget 100000000 \
    --args "0x_token" "amount"
```
