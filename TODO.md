# TODO

High-level milestones
- [ ] M1: Server skeleton, config load/validate, auth + origin checks, readiness line
- [ ] M2: fs_read + fs_write with path safety and unit tests
- [ ] M3: exec with allowlist, timeouts, output caps, streaming (NDJSON)
- [ ] M4: Typed error model, structured logging, audit completeness
- [ ] M5: Integration + property tests; golden I/O tests
- [ ] M6: launchd sample and docs

Working list
- [ ] Define typed errors and HTTP mapping
- [ ] Implement config structs (TOML) and validation
- [ ] Server: axum router, layers (auth, origin, rate limit), base_path
- [ ] `/mcp/capabilities` endpoint
- [ ] `/mcp/call` endpoint with stream/non-stream selection
- [ ] `/healthz` endpoint (auth enforced)
- [ ] Tool registry struct + registration of fs_read, fs_write, exec
- [ ] Path canonicalization with symlink containment check
- [ ] fs_read implementation
- [ ] fs_write implementation incl. optional POSIX mode
- [ ] exec implementation with allowlist resolution, pass_env, timeouts, caps, kill
- [ ] Streaming encoder for NDJSON
- [ ] Structured logging via tracing, redaction helpers
- [ ] Unit tests: paths, auth/origin, exec timeouts/caps
- [ ] Integration tests: end-to-end
- [ ] proptest for path normalization (behind feature if needed)
