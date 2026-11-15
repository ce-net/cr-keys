# cr-keys

The `cr.keys` full-stack cell of the control room (repo:control-room docs/DESIGN.md): one contract crate
`crates/cr-keys-api` (major 2, with its `views!` block; major 1 frozen in `src/versions/v1.rs` and still served),
one backend `crates/cr-keys`, and its view `ui/src/*.ts` against the generated `ui/gen/`. Apache-2.0.

`cargo test` runs the cell's tests (both majors); the view is built and type-checked by the site build in
repo:control-room (`ui/tools/build.mjs`).

Its own key: the backend reads its sources with the cell's key, never the node's. The node mints one grant for it,
exactly `GRANT` in `crates/cr-keys/src/lib.rs`, kept with the key in `<ce data dir>/cell-keys/cr.keys/`
(`identity/node.key`, `grant.hex`): `ce-iam --data-dir <that dir> whoami` makes the key, then
`ce-iam grant --to <its id> --nonce <n> --action <each of GRANT> > <that dir>/grant.hex` (revoke with `ce revoke <n>`).
`serve` refuses to start without them.
