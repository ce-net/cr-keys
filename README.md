# cr-keys

The `cr.keys` full-stack cell of the control room (repo:control-room docs/DESIGN.md): one contract crate
`crates/cr-keys-api` (major 2, with its `views!` block; major 1 frozen in `src/versions/v1.rs` and still served),
one backend `crates/cr-keys`, and its view `ui/src/*.ts` against the generated `ui/gen/`. Apache-2.0.

`cargo test` runs the cell's tests (both majors); the view is built and type-checked by the site build in
repo:control-room (`ui/tools/build.mjs`).
