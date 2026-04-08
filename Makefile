.PHONY: serve dev clean build test lint

serve:
	cargo run -- serve

dev:
	cargo watch -x 'run -- serve'

build:
	cargo build

test:
	cargo test

lint:
	cargo clippy -- -D warnings

clean:
	rm -f data/hermes.db data/hermes.db-wal data/hermes.db-shm
	find data/reports -type f ! -name '.gitkeep' -delete 2>/dev/null || true
	@echo "All data wiped"

reset: clean serve
