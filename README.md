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
| `docker` | Images für Coin-Daemons und den Test-Miner |
| `config/dev.toml` | Konfiguration für die lokale Umgebung |
| `reference/yiimp` | Yiimp-Quellcode zum Nachschlagen, nicht eingecheckt |

## Lokale Umgebung

Voraussetzungen: Docker, Rust, Node.js.

```bash
docker compose up -d --build --wait
```

Startet PostgreSQL (Port 55432), zwei Dogecoin-Nodes (RPC auf 18332) sowie je einen Xaya- und SpaceXpanse-Node (RPC auf 18493 und 18499), alle im Regtest-Modus. Alles ist nur an 127.0.0.1 gebunden.

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

Test-Miner (cpuminer) gegen den laufenden Stratum-Server starten, beenden mit Strg+C. Der Benutzername ist die Auszahlungsadresse, also zuerst eine Regtest-Adresse holen:

```bash
docker compose exec dogecoin dogecoin-cli -regtest -rpcport=18332 -rpcuser=pool -rpcpassword=pooldev getnewaddress
```

```bash
MINER_ADDRESS=<Adresse> docker compose --profile miner run --rm miner
```

Es gibt einen Stratum-Port pro Algo (Scrypt 3333, Neoscrypt-Xaya 3334). Laufen mehrere Coins auf einem Port, erkennt der Server die Coin an der Adresse; mit `c=<SYMBOL>` im Passwort legt der Miner sie selbst fest.

Für Xaya und SpaceXpanse gibt es einen eigenen Test-Miner; die Adresse kommt vom Xaya-Node:

```bash
docker compose exec xaya xaya-cli -regtest -rpcport=18493 -rpcuser=pool -rpcpassword=pooldev getnewaddress
```

```bash
MINER_ADDRESS=<Adresse> docker compose --profile miner run --rm miner-xaya
```

SpaceXpanse nutzt denselben Miner und Port mit einer Adresse vom `spacexpanse`-Node (`spacexpanse-cli`, RPC-Port 18499).

Solo- und Party-Mining wählt der Miner ebenfalls über das Passwort: `MINER_PASSWORD=m=solo` oder `MINER_PASSWORD=m=party.<Name>` vor denselben Befehl setzen. Mehrere Optionen werden mit Komma getrennt.

Block-Belohnungen werden erst nach 60 Bestätigungen gutgeschrieben und ausgezahlt. Auf Regtest erzeugt man die Bestätigungen mit `generate 61`.

Das Dashboard liest direkt aus der Datenbank. Ohne `DATABASE_URL` nutzt es im Entwicklungsmodus die lokale Dev-Datenbank; `POOL_STRATUM_HOST` setzt den Hostnamen, der Minern als Stratum-Adresse angezeigt wird.

JSON-API für Miner und Dritt-Tools:

| Endpunkt | Inhalt |
|---|---|
| `/api/pool` | Coins mit Pool- und Netzwerk-Statistik |
| `/api/blocks?limit=50` | zuletzt gefundene Blöcke |
| `/api/miners/<Adresse>` | Hashrate, Guthaben, Worker, Auszahlungen eines Miners |

## Admin-Bereich

Unter `/admin` liegt eine Übersicht für den Betreiber: Geldflüsse je Coin, Auszahlungen, Blöcke mit Aufteilung, Miner sowie Stratum-Server und Coin-Nodes. Sie zeigt nur an und löst nichts aus.

Der Zugang hängt an der Umgebungsvariable `ADMIN_PASSWORD` der Web-App (mindestens 12 Zeichen). Ist sie nicht gesetzt, ist der Bereich abgeschaltet. Für die lokale Entwicklung steht sie in `web/.env.local`, das nicht eingecheckt wird.

## Betrieb auf mehreren Servern

Der Pool besteht aus drei Diensten, die sich nur die Datenbank teilen:

| Dienst | Wie oft | Aufgabe |
|---|---|---|
| `pool-stratum` | einer je Region | nimmt Miner an, prüft Shares, reicht Blöcke ein |
| `pool-core` | genau einer | Abrechnung und Auszahlungen; eine zweite Instanz verweigert den Start |
| `web` | einer | Website und JSON-API |

Jeder Stratum-Server braucht eine eigene Nummer (`POOL_INSTANCE_ID`, 0 bis 255). Sie sorgt dafür, dass Miner auf verschiedenen Servern nie dieselbe Arbeit rechnen; eine doppelt vergebene Nummer wird beim Start abgelehnt. Die Server melden sich in der Datenbank an, und die Website bietet die erreichbaren zur Auswahl an.

`pool-stratum` und `pool-core` werden aus `docker/pool/Dockerfile` gebaut (Ziele `stratum` und `core`) und lesen `config/production.toml`. Geheimnisse und alles Server-Spezifische kommen aus Umgebungsvariablen:

| Variable | Dienst | Inhalt |
|---|---|---|
| `DATABASE_URL` | alle | PostgreSQL-Verbindung |
| `POOL_INSTANCE_ID` | Stratum | eindeutige Nummer des Servers |
| `POOL_REGION` | Stratum | Anzeigename für Miner, z. B. „Europe“ |
| `POOL_STRATUM_HOST` | Stratum | öffentlicher Hostname des Servers |
| `POOL_RPC_PASSWORD_<SYMBOL>` | Stratum, Core | RPC-Passwort des Coin-Nodes |
| `POOL_RPC_URL_<SYMBOL>` | Stratum, Core | optional, abweichende Adresse des Coin-Nodes |
| `POOL_ADDRESS_<SYMBOL>` | Stratum | Adresse des Pool-Wallets für Block-Belohnungen |
| `ADMIN_PASSWORD` | Web | Passwort für den Admin-Bereich |

Die Uhren aller Server müssen per NTP synchron laufen, weil Shares und Blöcke über ihren Zeitstempel zugeordnet werden.

Tests:

```bash
cargo test --workspace
```

Eine andere Konfigurationsdatei wählt man über die Umgebungsvariable `POOL_CONFIG`.

Yiimp-Referenz holen:

```bash
git clone --depth 1 https://github.com/tpruvot/yiimp reference/yiimp
```
