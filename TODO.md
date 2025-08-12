# TODO

High-level milestones
- [x] M1: Server skeleton, config load/validate, auth + origin checks, readiness line
- [x] M2: fs_read + fs_write with path safety
- [x] M3: exec with allowlist, timeouts, output caps, streaming (NDJSON)
- [ ] M4: Typed error model, structured logging, audit completeness
- [ ] M5: Integration + property tests; golden I/O tests
- [x] M6: launchd sample and docs

Working list
- [ ] Define typed errors and HTTP mapping
- [ ] Implement config structs (TOML) and validation
- [x] Server: axum router, base_path
- [x] `/mcp/capabilities` endpoint
- [x] `/mcp/call` endpoint with stream/non-stream selection
- [x] `/healthz` endpoint (auth enforced)
- [x] Tool registry struct + registration of fs_read, fs_write, exec
- [x] Path canonicalization with symlink containment check
- [x] fs_read implementation
- [x] fs_write implementation incl. optional POSIX mode
- [x] exec implementation with allowlist resolution, pass_env, timeouts, caps, kill
- [x] Streaming encoder for NDJSON
- [x] Request body size limiter (layer) per config
- [x] Rate limiting: global + per-token
- [ ] Structured audit logging: request_id, sizes, outcome, timing, redactions
- [ ] Unit tests: paths, auth/origin, exec timeouts/caps
- [ ] Integration tests: end-to-end
- [ ] proptest for path normalization (behind feature if needed)
