## Setup

### Local development
- install the `cbt` CLI tool
```sh
gcloud components install cbt
```
- start the emulator
```sh
gcloud beta emulators bigtable start
```
- set `BIGTABLE_EMULATOR_HOST` environment variable
```sh
$(gcloud beta emulators bigtable env-init)
```
- Run `./src/bigtable/init.sh` to configure the emulator