.PHONY: dev build test test-e2e test-backend test-frontend clippy clean

# ── Development ──────────────────────────────────────────────────────────
dev:
	@echo "Starting backend (cargo watch) + frontend (bun dev)…"
	@echo "Backend will listen on http://localhost:3000"
	@echo "Frontend dev server on http://localhost:5173 (proxies /api and /ws)"
	@trap 'kill 0' EXIT; \
		(cargo watch -w src -x run 2>&1 | sed 's/^/[backend] /') & \
		(cd frontend && bun dev 2>&1 | sed 's/^/[frontend] /') & \
		wait

# ── Build ─────────────────────────────────────────────────────────────────
build:
	./scripts/build.sh

build-debug:
	./scripts/build.sh --debug

build-frontend:
	cd frontend && bun install --frozen-lockfile && bun run build

build-backend:
	cargo build --release

# ── Test ──────────────────────────────────────────────────────────────────
test:
	cargo test
	cd frontend && bun test

test-backend:
	cargo test

test-frontend:
	cd frontend && bun test

test-e2e:
	cd frontend && bun run test:e2e

clippy:
	cargo clippy --all-targets -- -D warnings

# ── Clean ─────────────────────────────────────────────────────────────────
clean:
	cargo clean
	rm -rf frontend/dist
	rm -rf frontend/node_modules/.cache

clean-all: clean
	rm -rf frontend/node_modules
