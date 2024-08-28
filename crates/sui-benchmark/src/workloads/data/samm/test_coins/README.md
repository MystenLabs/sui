# Test coins

## cmd

```bash
# deploy on sui devnet 0.18
sui client publish --gas-budget 10000
package=0x5f205364e20114075512028e2c3976bbaaa5f482
faucet=0x37019bd3aa332a1ee442c76c6ceaf9390a6e99de
USDT="$package::coins::USDT"
XBTC="$package::coins::XBTC"
# require deployed swap
swap_global=0x28ae932ee07d4a0881e4bd24f630fe7b0d18a332

# add faucet admin
sui client call \
  --gas-budget 10000 \
  --package $package \
  --module faucet \
  --function add_admin \
  --args $faucet \
      0x4d7a8549beb8d9349d76a71fd4f479513622532b

# claim usdt
sui client call \
  --gas-budget 10000 \
  --package $package \
  --module faucet \
  --function claim \
  --args $faucet \
  --type-args $USDT

# force claim xbtc with amount
# 10 means 10*ONE_COIN
sui client call \
  --gas-budget 10000 \
  --package $package \
  --module faucet \
  --function force_claim \
  --args $faucet 10 \
  --type-args $XBTC

# add new coin supply
PCX_CAP=0xfe6db5a5802acb32b566d7b7d1fbdf55a496eb7f
PCX="0x44984b1d38594dc64a380391359b46ae4207d165::pcx::PCX"
sui client call \
  --gas-budget 10000 \
  --package $package \
  --module faucet \
  --function add_supply \
  --args $faucet \
         $PCX_CAP \
  --type-args $PCX
  
# force add liquidity
sui client call \
  --gas-budget 100000 \
  --package $package \
  --module faucet \
  --function force_add_liquidity \
  --args $faucet $swap_global
```
