# Archive

Simple module to showcase archive with a table + reverse lookup.

```bash
sui client publish --gas-budget 100000000


# ...

sui client call \
    --package $PACKAGE \
    --module archive \
    --function add_record \
    --gas-budget 100000000 \
    --args \
        $ARCHIVE \ # archive object from init
        0x6 \ # clock
        0xA11CE \ # marker
        "war and peace" # name
```
