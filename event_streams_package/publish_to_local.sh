# 1. Check that `sui client active-env` is local
# 2. Get gas from faucet (`sui client faucet`)

sui=../target/release/sui

$sui client switch --env local

$sui client switch --address optimistic-emerald

$sui client faucet

sleep 5

$sui client publish --gas-budget 50000000

# sui client ptb --move-call "$PKG::event_streams_package::create" optimistic-emerald

# sui client ptb --move-call "$PKG::event_streams_package::emit_event" @obj 10