# rust-maker-chaser

Standalone Rust Maker chaser for Binance USDⓈ-M Futures. Places Post-Only (`GTX`) limit orders and chases the best bid/ask over the bookTicker WebSocket.

The Cargo package name is still `rust-chaser` (binary crate). This repository is the source of truth; it no longer depends on the trading-dashboard monorepo.

## Requirements

- Rust toolchain (edition 2021)
- Binance Futures API key and secret with trading permission for the account you intend to use

## Quick start

```bash
git clone https://github.com/armandorgm/rust-maker-chaser.git
cd rust-maker-chaser
cp .env.example .env
```

Edit `.env` with your credentials, then:

```bash
cargo run --release
```

On Windows the app opens a small always-on-top GUI (`Quick Maker Chaser`).

## Environment variables

| Variable | Required | Description |
|---|---|---|
| `BINANCE_API_KEY` | yes | Futures API key |
| `BINANCE_API_SECRET` | yes | Futures API secret |
| `TESTNET` | no | `true` / `false` (default `false`) |

Lookup order:

1. `.env` in the current working directory (crate root)
2. `../backend/.env` (optional fallback if you still run next to the old dashboard)
3. `backend/.env` (same fallback)
4. Process environment via `dotenvy`

Never commit `.env`. The file is gitignored.

## Symbol

The chaser is currently hardcoded to `1000PEPEUSDC` (USDⓈ-M).

## License

No license file is attached yet. Treat the code as unpublished for reuse until one is added.
