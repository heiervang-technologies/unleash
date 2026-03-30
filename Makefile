.PHONY: install uninstall build clean test

install:
	./scripts/install.sh

uninstall:
	./scripts/uninstall.sh

build:
	cargo build --release

clean:
	cargo clean

test:
	cargo test
