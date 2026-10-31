# HTTP API

> Source: ORIGINAL_BRIEF.md §8

## 8. HTTP API (illustrative, subject to validation)

```http
GET  /api/v1/status
GET  /api/v1/device
GET  /api/v1/device/identity
GET  /api/v1/measurements
GET  /api/v1/registers
GET  /api/v1/registers/catalog
GET  /api/v1/registers/discovered
GET  /api/v1/diagnostics
GET  /api/v1/events
POST /api/v1/discovery/scan
GET  /api/v1/discovery/status
POST /api/v1/controls/temporary-lux
POST /api/v1/controls/heating-offset
GET  /openapi.json
GET  /health
```

Use typed errors, documented schemas, authentication, command authorization, stale-state reporting, and live updates. Favor domain controls rather than raw register writes. Control endpoints remain disabled until physically verified.
