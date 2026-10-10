# Mining Pool

Multi-Coin-Mining-Pool, neu aufgebaut mit [Yiimp](https://github.com/tpruvot/yiimp) als Referenz.

## Aufbau

| Pfad | Inhalt |
|---|---|
| `crates/stratum` | Stratum-Server (Rust, tokio) |
| `crates/core` | Pool-Kern: Blöcke, Abrechnung, Ledger, Auszahlung |
| `crates/common` | Konfiguration und Coin-Daemon-RPC |
| `migrations` | PostgreSQL-Schema, wird von `pool-core` beim Start angewendet |
| `web` | Dashboard (Next.js) |
| `docker` | Images für Coin-Daemons |
| `config/dev.toml` | Konfiguration für die lokale Umgebung |
| `reference/yiimp` | Yiimp-Quellcode zum Nachschlagen, nicht eingecheckt |

## Lokale Umgebung

Voraussetzungen: Docker, Rust, Node.js.

```bash
docker compose up -d --build --wait
```

Startet PostgreSQL (Port 55432), Redis (56379) und zwei Dogecoin-Nodes im Regtest-Modus (RPC auf 18332). Alles ist nur an 127.0.0.1 gebunden.

Auf einer frischen Regtest-Chain einmalig Blöcke erzeugen, sonst liefert der Node keine Block-Templates:

```bash
docker compose exec dogecoin dogecoin-cli -regtest -rpcport=18332 -rpcuser=pool -rpcpassword=pooldev generate 101
```

Dienste starten:

```bash
cargo run -p pool-core
```

```bash
cargo run -p pool-stratum
```

```bash
npm --prefix web run dev
```

Tests:

```bash
cargo test --workspace
```

Eine andere Konfigurationsdatei wählt man über die Umgebungsvariable `POOL_CONFIG`.

Yiimp-Referenz holen:

```bash
git clone --depth 1 https://github.com/tpruvot/yiimp reference/yiimp
```
