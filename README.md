# srvcs-log10

The base-10 logarithm primitive of the srvcs.cloud distributed standard library.

Its single concern: **compute the base-10 logarithm of a number** (`log10`). It
does not validate input itself — it delegates "is this a number" to
[`srvcs-isnumber`](https://github.com/srvcs/isnumber) over HTTP, the single
source of truth for that question.

This is a **floating-point** service: both integers and floats are valid input,
and the `result` is an `f64` (a JSON number that may have a fractional part). So
`log10(1000) == 3.0`, `log10(1) == 0.0`, and `log10(2) == 0.30102999566398...`.

The base-10 logarithm is undefined over the reals for non-positive inputs, so a
`value <= 0.0` is rejected with a **422 domain error**.

If `srvcs-isnumber` is unreachable, `srvcs-log10` reports itself **degraded
(503)** rather than guessing.

## API

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/` | Service identity, concern, and dependency list |
| `POST` | `/` | Compute `log10(value)` |
| `GET` | `/healthz` `/readyz` `/metrics` `/openapi.json` | srvcs service standard surface |

```sh
curl -s -X POST localhost:8080/ -H 'content-type: application/json' -d '{"value": 1000}'
# {"value":1000,"result":3.0}
```

Responses:

- `200 {"value": n, "result": float}` — evaluated.
- `422` — the value is not a number (per `srvcs-isnumber`), or is non-positive
  (`{"error":"logarithm of a non-positive number"}`).
- `503` — a dependency is unavailable.

## Dependencies

- [`srvcs-isnumber`](https://github.com/srvcs/isnumber) — input validation.

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `SRVCS_BIND_ADDR` | `0.0.0.0:8080` | Bind address |
| `SRVCS_ISNUMBER_URL` | `http://127.0.0.1:8081` | Base URL of `srvcs-isnumber` |
| `SRVCS_ENV` | `development` | Environment label for logs |
| `RUST_LOG` | `info,tower_http=info` | Tracing filter |

## Local checks

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```
