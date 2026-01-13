.PHONY: install build clean test

install:
	cargo install --path .

build:
	cargo build --release

clean:
	cargo clean

test:
	cargo test
