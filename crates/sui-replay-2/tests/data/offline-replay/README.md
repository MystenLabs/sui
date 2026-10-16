# Offline mainnet replay corpus

These mainnet transactions are the offline test corpus for `sui-replay-2`.  The integration test
replays the complete list twice with the checked-in `FileSystemStore` cache: once with a fresh
executor per transaction and once with `--cache-executor`, both in `fs-only` mode.  To add one,
append its digest to `digests.txt`, run the capture command below, and add its row. To remove one,
delete its digest and row; rebuild the cache from an empty root with the remaining list if its
cached data should be pruned.

| Epoch | Protocol | Checkpoint | Effects | PTB shape | Digest |
| ---: | ---: | ---: | --- | --- | --- |
| 1 | 4 | 9,770 | success | `ConsensusCommitPrologue` (not a PTB) | [E6Ye42piuJUrensN8ovT2EnisYXRN9zYUyXinrfJDoHR](https://devxplorer.io/?search=E6Ye42piuJUrensN8ovT2EnisYXRN9zYUyXinrfJDoHR&network=mainnet) |
| 16 | 6 | 1,193,836 | success | `Publish → TransferObjects`; inputs: pure; coin gas | [8TiNvKYxDRv2fBk75m1JxHiSxacUj6yqAvrDy3qroM8v](https://devxplorer.io/?search=8TiNvKYxDRv2fBk75m1JxHiSxacUj6yqAvrDy3qroM8v&network=mainnet) |
| 20 | 9 | 1,566,040 | success | `MoveCall(package::authorize_upgrade) → Upgrade → MoveCall(package::commit_upgrade)`; inputs: pure, owned/immutable; coin gas | [BNMKnE9STjyAaVRSUTwb9viEKrxBe8BNq4PZEiLvZnkT](https://devxplorer.io/?search=BNMKnE9STjyAaVRSUTwb9viEKrxBe8BNq4PZEiLvZnkT&network=mainnet) |
| 50 | 11 | 4,013,666 | success | `SplitCoins → TransferObjects`; inputs: pure; coin gas | [KRpMVYCS6iZ128f3XZVE2C9WeeuYuNQk1g8vW5n6RmZ](https://devxplorer.io/?search=KRpMVYCS6iZ128f3XZVE2C9WeeuYuNQk1g8vW5n6RmZ&network=mainnet) |
| 100 | 18 | 8,267,272 | success | `MoveCall(game_8192::make_move)`; inputs: pure, owned/immutable; coin gas | [2dnfecDkhJA1qFmxfcccjas9fLoLXE5bf97TNj9mGsrv](https://devxplorer.io/?search=2dnfecDkhJA1qFmxfcccjas9fLoLXE5bf97TNj9mGsrv&network=mainnet) |
| 150 | 22 | 12,538,237 | success | `SplitCoins → MoveCall(coin_flip_v2::start_game_with_kiosk)`; inputs: pure, mutable shared, immutable shared; coin gas | [2seYwy3SpAfBuGJeoAH67UYDT4MhuBKchLn1eT3yEEb6](https://devxplorer.io/?search=2seYwy3SpAfBuGJeoAH67UYDT4MhuBKchLn1eT3yEEb6&network=mainnet) |
| 250 | 32 | 21,221,613 | success | `MoveCall(vaa::parse_and_verify) → MoveCall(pyth::create_authenticated_price_infos_using_accumulator) → SplitCoins → 3× MoveCall(pyth::update_single_price_feed) → MoveCall(hot_potato_vector::destroy)`; inputs: pure, mutable shared, immutable shared; coin gas | [5p1z99hXrgFqXnXhVdSW8PuLTzeCwDG4ssRRn3wq1fo](https://devxplorer.io/?search=5p1z99hXrgFqXnXhVdSW8PuLTzeCwDG4ssRRn3wq1fo&network=mainnet) |
| 350 | 38 | 29,887,400 | success | `MoveCall(clob_v2::cancel_order)`; inputs: pure, owned/immutable, mutable shared; coin gas | [F1hAsRTu55SCB3F4wHUA6XeMQry6ftmPwhteq1F49SeN](https://devxplorer.io/?search=F1hAsRTu55SCB3F4wHUA6XeMQry6ftmPwhteq1F49SeN&network=mainnet) |
| 450 | 49 | 38,552,340 | failure | `MoveCall(staked_sui_vault::epoch_was_changed)`; inputs: pure, mutable shared, immutable shared; coin gas | [DdsMhw8Y2SpCMucUqNnmw8XG226Xp2Rbt3SGepjgVJaB](https://devxplorer.io/?search=DdsMhw8Y2SpCMucUqNnmw8XG226Xp2Rbt3SGepjgVJaB&network=mainnet) |
| 550 | 60 | 68,577,193 | success | `46× MoveCall(fomo::mine) → TransferObjects`; inputs: pure, owned/immutable, mutable shared, immutable shared; coin gas | [GJaNHCzqEezwhcVrViARAZT3fna7oFNgCA5FusneZYG4](https://devxplorer.io/?search=GJaNHCzqEezwhcVrViARAZT3fna7oFNgCA5FusneZYG4&network=mainnet) |
| 650 | 71 | 104,022,145 | success | `SplitCoins → TransferObjects`; inputs: pure, owned/immutable; coin gas | [13P2GB3vgMQgZ5efACjTScEYNQ8LtymrWfJYJiWgovUB](https://devxplorer.io/?search=13P2GB3vgMQgZ5efACjTScEYNQ8LtymrWfJYJiWgovUB&network=mainnet) |
| 700 | 74 | 121,907,118 | failure | `MoveCall(staked_sui_vault::epoch_was_changed)`; inputs: pure, mutable shared, immutable shared; coin gas | [2bVyS9thJ5UdEBie6QWdVCT2KfV6q5W2oNpza1Sr7poi](https://devxplorer.io/?search=2bVyS9thJ5UdEBie6QWdVCT2KfV6q5W2oNpza1Sr7poi&network=mainnet) |
| 750 | 81 | 140,199,868 | success | `MoveCall(game_wallet::update_ledger)`; inputs: pure, owned/immutable; coin gas | [NjpBzyhGWkcYCMdVZXBLSAEGvNpPk9YCqRu9xxJeThH](https://devxplorer.io/?search=NjpBzyhGWkcYCMdVZXBLSAEGvNpPk9YCqRu9xxJeThH&network=mainnet) |
| 800 | 85 | 158,847,769 | success | `MoveCall(safeV2::withdraw) → MoveCall(swap_router::swap_b_a_with_partner) → 2× MoveCall(safeV2::deposit)`; inputs: pure, mutable shared, immutable shared; coin gas | [232zifSt4KR9kpX2KsHJLGPAoD86K42RD1eYWdep9EKW](https://devxplorer.io/?search=232zifSt4KR9kpX2KsHJLGPAoD86K42RD1eYWdep9EKW&network=mainnet) |
| 900 | 95 | 194,936,778 | success | `ConsensusCommitPrologueV4` (not a PTB) | [kobSH4poi5Pvw9zYK7M4BGo1yCLePoNpQYRnG8Nzquu](https://devxplorer.io/?search=kobSH4poi5Pvw9zYK7M4BGo1yCLePoNpQYRnG8Nzquu&network=mainnet) |
| 950 | 101 | 213,219,977 | failure | `MoveCall(fl::fb) → MoveCall(router::new_swap_context) → MoveCall(suidex::swap) → MoveCall(cetus::swap) → MoveCall(hasui::swap) → MoveCall(router::confirm_swap) → MoveCall(fl::rb) → MoveCall(fl::e)`; inputs: pure, mutable shared, immutable shared; coin gas | [AeShJAvFtHgCyAhr4T4hooE6Uj33MCP486TZmfUNgqXN](https://devxplorer.io/?search=AeShJAvFtHgCyAhr4T4hooE6Uj33MCP486TZmfUNgqXN&network=mainnet) |
| 1,000 | 104 | 231,749,801 | success | `MoveCall(coin::zero) → MoveCall(pool_script_v2::collect_reward) → MoveCall(coin::zero) → MoveCall(pool_script_v2::collect_reward) → MoveCall(pool_script::close_position)`; inputs: pure, owned/immutable, mutable shared, immutable shared; coin gas | [Hq5nnfnctx5LQHDqU4JpeHbDrEFf1dQFEdZPVBoJY7nK](https://devxplorer.io/?search=Hq5nnfnctx5LQHDqU4JpeHbDrEFf1dQFEdZPVBoJY7nK&network=mainnet) |
| 1,050 | 111 | 248,753,276 | success | `MoveCall(entrypoint::aaf6cc9d45ba0185f)`; inputs: pure, mutable shared, immutable shared; coin gas | [3NMkFnT697muMPFAY2McHGA3rLXbp2TWreEXkXGhr8G2](https://devxplorer.io/?search=3NMkFnT697muMPFAY2McHGA3rLXbp2TWreEXkXGhr8G2&network=mainnet) |
| 1,100 | 120 | 265,747,329 | failure | `MoveCall(sq::csst) → MoveCall(dbke::elf)`; inputs: pure, mutable shared, immutable shared; coin gas | [BW3UdJP2CuKDcGqk1WUzpwdDbQrPJp5aM3ik6xKruYhg](https://devxplorer.io/?search=BW3UdJP2CuKDcGqk1WUzpwdDbQrPJp5aM3ik6xKruYhg&network=mainnet) |
| 1,150 | 124 | 283,615,954 | success | `MoveCall(balance_manager::generate_proof_as_owner) → MoveCall(pool::cancel_live_orders)`; inputs: pure, mutable shared, immutable shared; coin gas | [XGtJ6JEeLZKuYdmZL2EmC5BCDnVojHjYtqhe1TUHSAH](https://devxplorer.io/?search=XGtJ6JEeLZKuYdmZL2EmC5BCDnVojHjYtqhe1TUHSAH&network=mainnet) |
| 1,200 | 129 | 302,995,338 | success | `ProgrammableSystemTransaction` (not a PTB) | [nagoVxhpZDBvkRaSFdvV958WZg2K6kVS9vJye896JvV](https://devxplorer.io/?search=nagoVxhpZDBvkRaSFdvV958WZg2K6kVS9vJye896JvV&network=mainnet) |
| 1,250 | 136 | 322,334,899 | failure | `MoveCall(nonce_manager::commit) → MoveCall(c::cetus_b2a)`; inputs: pure, mutable shared, immutable shared; coin gas | [ATCkxsd3cySF8xNS5VoEZrbzVp1FYe4seFyJYBaSF7q](https://devxplorer.io/?search=ATCkxsd3cySF8xNS5VoEZrbzVp1FYe4seFyJYBaSF7q&network=mainnet) |
| 1,250 | 136 | 322,334,899 | success | `MakeMoveVec → MoveCall(interface::try_cancel_orders) → MoveCall(option::none) → MoveCall(interface::start_perpetuals_session) → MoveCall(clearing_house::place_limit_order) → MoveCall(interface::end_perpetuals_session) → MoveCall(interface::deallocate_collateral_from_position) → MoveCall(clearing_house::share)`; inputs: pure, mutable shared, immutable shared; address-balance gas; dynamic-field effects | [2jdmBKvCUTEFZddnbHQmaBQST3BDexKYfNqDK1qDuogt](https://devxplorer.io/?search=2jdmBKvCUTEFZddnbHQmaBQST3BDexKYfNqDK1qDuogt&network=mainnet) |
| 1,250 | 136 | 322,337,399 | success | `MoveCall(coin::redeem_funds) → SplitCoins → MoveCall(coin::send_funds) → MoveCall(limbo::play_v2) → TransferObjects`; inputs: pure, mutable shared, immutable shared, `FundsWithdrawal(SUI, Sender, MaxAmountU64)`; address-balance gas | [4LKy4LrGLKsy3CndN6FMDtmPyJhuFQPhM19P84MprAVr](https://devxplorer.io/?search=4LKy4LrGLKsy3CndN6FMDtmPyJhuFQPhM19P84MprAVr&network=mainnet) |
| 1,250 | 136 | 322,337,899 | success | `MergeCoins → SplitCoins → MoveCall(wheel::play_v2) → TransferObjects`; inputs: pure, owned/immutable, mutable shared, immutable shared; coin gas | [5nUQabAosL96ASjfeBvzbaQNaXQEkHH6AXeLifRFwfWX](https://devxplorer.io/?search=5nUQabAosL96ASjfeBvzbaQNaXQEkHH6AXeLifRFwfWX&network=mainnet) |
| 1,252 | 136 | 323,439,438 | failure | `MoveCall(tto_demo::uid_mut) → MoveCall(transfer::public_receive) → TransferObjects`; inputs: pure, owned/immutable, receiving; address-balance gas | [DuWXhNqZ1Cy9DjD4VdvNygR42eM9BaDckU6itrKFshrn](https://devxplorer.io/?search=DuWXhNqZ1Cy9DjD4VdvNygR42eM9BaDckU6itrKFshrn&network=mainnet) |
| 1,275 | 138 | 331,854,382 | success | `ProgrammableSystemTransaction` (not a PTB) | [BkcuC2ZhhabUwNnZnJS1Rrs9cUuxGc7d7ENFPBfv8qre](https://devxplorer.io/?search=BkcuC2ZhhabUwNnZnJS1Rrs9cUuxGc7d7ENFPBfv8qre&network=mainnet) |

Capture additions through the network, then prove the resulting corpus is complete offline:

```sh
SUI_DATA_STORE="$PWD/crates/sui-replay-2/tests/data/offline-replay" \
  cargo run -p sui-replay-2 -- \
    --digests-path crates/sui-replay-2/tests/data/offline-replay/digests.txt \
    --store-mode fs-then-gql --skip-artifacts --terminate-early

SUI_SKIP_SIMTESTS=1 cargo nextest run -p sui-replay-2
```
