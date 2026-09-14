# Daily ops (Oracle)

Oneshot + timer. The OS is the scheduler. Do not add an in-process cron.

Operator logs: `tracing` on stderr → journald (`SyslogIdentifier=edgar-8k-labels-ingest`). Default `RUST_LOG=info`.

`ingest_runs` is capturable domain telemetry. Query it in mosaic.

## Layout

| | |
| --- | --- |
| Prefix | `/opt/edgar-8k-labels` |
| State | `/var/lib/edgar-8k-labels/edgar-8k-labels.sqlite` |
| User | `edgar` |
| Env | `/opt/edgar-8k-labels/etc/edgar-8k-labels.env` (`chmod 600`) |
| Timer | `edgar-8k-labels-ingest.timer` **07:30 UTC** + 15m jitter, `Persistent=true` |

No published `current/`. Do not copy a laptop sqlite onto the host.

## Install

```bash
cargo build --release
sudo ./deploy/install.sh
# set SEC_USER_AGENT in /opt/edgar-8k-labels/etc/edgar-8k-labels.env
```

`install.sh` enables the timer **without** `--now`. First run: `sudo systemctl start edgar-8k-labels-ingest.service`.

## Timer failed

1. `systemctl list-failed --no-pager`
2. `journalctl -u edgar-8k-labels-ingest.service -n 80 --no-pager`
3. `edgar-8k-labels --db /var/lib/edgar-8k-labels/edgar-8k-labels.sqlite status`
4. Re-run: `sudo systemctl start edgar-8k-labels-ingest.service`

Do not hand-edit sqlite.

Weekend / US holiday master-index **404 is success** (`status=ok`, zero filings). From this OCI IP an unpublished weekend path is often **403** rather than 404 — Sat/Sun 403 is the same success. Weekday index 403 is `status=error` (UA/Akamai). Index 5xx after retries is `status=error` (unit failed). Some filings 403/404 with submissions fallback is `partial` (exit 0).

## SEC fair access

Official: [Accessing EDGAR Data](https://www.sec.gov/search-filings/edgar-search-assistance/accessing-edgar-data).

- `SEC_USER_AGENT` sample shape: `edgar-8k-labels you@real-domain`
- Refuse `example.com` and github-paren UAs (Akamai 403 undeclared bot)
- Default sleep 0.5s (~2 req/s). Ceiling 10 req/s. Do not rotate User-Agents (cap is per IP)
- Two 403s: undeclared-bot UA vs datacenter IP reputation. This host already GETs `company_tickers_exchange.json` for tail-to-ticker. Filing `.txt` 403 falls back to `data.sec.gov/submissions/CIK*.json`. Still no HTML.

Do not send `FAA_USER_AGENT` to SEC. Do not download `Feed/*.nc.tar.gz`.
