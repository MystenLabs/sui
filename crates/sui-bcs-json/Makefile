main: examples/main.c rustlib bindings
	cc $< -o examples/$@ -L target/debug -l sui_bcs_json -l{pthread,dl,m}
	
.PHONY: rustlib
rustlib:
	cargo test && cargo build --target-dir target

bindings:
	cargo test && cargo build --target-dir target
	cp sui_bcs_json_bindings.h examples/

example: examples/main.c
	# cc $< -o examples/main -L target/debug -l sui_bcs_json -l{pthread,dl,m}
	cc $< -o examples/main -L target/debug -l sui_bcs_json -l{pthread,dl,m}

clean:
	cargo clean
	$(RM) main
