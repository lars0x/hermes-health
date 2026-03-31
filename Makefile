.PHONY: serve clean build test

serve:
	cargo run -- serve

build:
	cargo build

test:
	cargo test

clean:
	rm -f data/hermes.db data/hermes.db-wal data/hermes.db-shm
	find data/reports -type f ! -name '.gitkeep' -delete 2>/dev/null || true
	@echo "All data wiped"

reset: clean serve
