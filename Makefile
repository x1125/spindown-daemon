.PHONY: build install

build:
	cargo build --release
	strip target/release/spindown-daemon

install: build
	sudo cp target/release/spindown-daemon /usr/local/bin/
	sudo chmod +x target/release/spindown-daemon