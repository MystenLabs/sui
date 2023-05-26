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
$(CACHE_DIR)/bin/rustup-init:
	$(call toolchain,' \
		wget "$(RUSTUPINIT_URL)" -O $@; \
	')

$(CACHE_DIR)/bin/rustup: $(CACHE_DIR)/bin/rustup-init
	$(call toolchain,' \
    	./$(CACHE_DIR)/bin/rustup-init \
			-y \
			--no-modify-path \
			--profile minimal \
			--default-toolchain $(RUST_VERSION) \
			--default-host $(RUST_ARCH); \
    	chmod -R a+w $(CACHE_DIR)/bin/rustup $(CACHE_DIR); \
	')

$(OUT_DIR)/sui-node:
	$(call toolchain,' \
		export RUSTFLAGS='-C target-feature=+crt-static' \
		&& cargo build \
			--target x86_64-unknown-linux-gnu \
			--locked \
			--release \
			--bin sui-node \
		&& cp target/x86_64-unknown-linux-gnu/release/sui-node /home/build/$@ \
	')
