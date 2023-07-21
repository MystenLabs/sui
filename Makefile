ifneq ("$(wildcard $(ROOT)/src/toolchain)","")
	clone := $(shell git submodule update --init --recursive)
endif
# TODO: Move all code to src (preferred) or add root path support to toolchain
include $(PWD)/src/toolchain/Makefile

.DEFAULT_GOAL :=
.PHONY: default
default: \
	toolchain \
	$(DEFAULT_GOAL) \
	$(OUT_DIR)/sui-node

# TODO: Eliminate rustup ASAP which has very weak supply chain integrity
# Favor signed/reproducible rust toolchain such as that from arch or debian
$(FETCH_DIR)/rustup-init:
	$(call toolchain,' \
		mkdir -p $(CACHE_DIR)/bin; \
		curl "$(RUSTUPINIT_URL)" --output $@; \
		chmod +x $@; \
	')

$(FETCH_DIR)/rocksdb.tgz:
	@$(call fetch_file,$(ROCKSDB_URL),$(ROCKSDB_HASH))

$(CACHE_DIR)/rocksdb-src/Makefile: \
	$(FETCH_DIR)/rocksdb.tgz
	tar -xzf $< -C $(CACHE_DIR)/
	mv $(CACHE_DIR)/facebook-rocksdb* $(dir $@)

$(CACHE_DIR)/lib/librocksdb.a: \
	$(CACHE_DIR)/rocksdb-src/Makefile
	$(call toolchain,' \
		$(MAKE) \
			--directory=$(CACHE_DIR)/rocksdb-src \
			-j$(CPUS) \
			static_lib \
	')

$(CACHE_DIR)/bin/rustup: $(FETCH_DIR)/rustup-init
	$(call toolchain,' \
    	./$(FETCH_DIR)/rustup-init \
			-y \
			--no-modify-path \
			--profile minimal \
			--default-toolchain $$RUST_VERSION \
			--default-host $$RUST_ARCH \
		&& source "/home/build/cache/x86_64/cargo/env" \
		&& rustup target add x86_64-unknown-linux-musl \
	')

$(OUT_DIR)/sui-node: \
	$(CACHE_DIR)/bin/rustup
	$(call toolchain,' \
		source "/home/build/cache/x86_64/cargo/env" \
		&& export RUSTFLAGS="-C target-feature=+crt-static" \
		&& export ROCKSDB_COMPILE=true \
		&& export ROCKSDB_STATIC=true \
		&& sudo cp /lib/x86_64-linux-gnu/libgcc_s.so.1 /lib64/ \
		&& cargo build \
			--target x86_64-unknown-linux-musl \
			--locked \
			--release \
			--bin sui-node \
		&& cp target/x86_64-unknown-linux-musl/release/sui-node /home/build/$@ \
	')
