//! FROZEN: cr.keys major 1, the first edition's contract, byte for byte (schema 0xf4f284ea9d9f783e). Served beside
//! major 2 until the first edition's face is gone and `Callers` has been zero for 14 days. Never edited.
//!
//! `cr.keys`: the Keys panel's view cell. Key metadata only: name, service, holders, expiry, rotation state, version,
//! short hash and last check. `KeyRow` has no field that can carry a key's value, and the cell's own grant on `keys`
//! has no `keys:read`, so a value can neither be asked for nor returned.
//!
//! Rights: `cr.keys:read` for `List`/`One`. Rows may be scoped with `cr.keys:scope:service=<glob>` and
//! `cr.keys:scope:owner=<glob>` abilities in the same chain. Holders need `cr.keys:holders`; without it they come back
//! empty and the view says `narrowed`.

ce_wire::api! {
    api CrKeys = "cr.keys", version 1;
    types {
        /// Health contract: these three fields, in this order, in every cell.
        pub struct Health { pub ok: bool, pub version: String, pub uptime_s: u64 }
        /// How a source cell answered this view: live, stale (age of the copy shown), down (why), or refused (the right
        /// this cell lacks on it).
        pub enum SourceState { Live, Stale { age_s: u64 }, Down { why: String }, Refused { needs: String } }
        pub struct Source { pub cell: String, pub node: String, pub state: SourceState, pub ms: u32 }
        /// Fresh: every share lasts more than the renewal window. DueSoon/Overdue: the first share ends in / ended
        /// `days` ago. Failed: the last key check failed. Manual: no share ends (nothing renews it on a clock).
        pub enum RotationState { Fresh, DueSoon { days: u32 }, Overdue { days: u32 }, Failed { why: String }, Manual }
        /// A home holding a share of the key: its name, its node id (hex), the lanes it may open it for, and when the
        /// share ends (unix seconds).
        pub struct Holder { pub name: String, pub home: String, pub lanes: Vec<String>, pub not_after: i64 }
        pub struct KeyRow {
            pub name: String, pub service: String, pub purpose: String, pub owner: String,
            pub version: i64, pub sha256_8: String, pub written_by: String, pub written_at: i64,
            pub expires_at: Option<i64>, pub days_left: Option<i64>, pub rotation: RotationState, pub self_renewing: bool,
            pub holders: Vec<Holder>, pub holder_count: u32, pub last_opened: i64, pub last_check: String, pub last_check_at: i64,
            pub refs: Vec<String>
        }
        /// q: substring of name, service or purpose ("" = all). due_within_days: only keys whose first share ends
        /// within that many days.
        pub struct KeysAsk { pub q: String, pub due_within_days: Option<u32> }
        /// narrowed: this viewer's chain removed rows or fields (a flag, never a count). can_ask: rights that would
        /// show more. call_id: the trace of this answer in ui.trace.
        pub struct KeysView {
            pub rows: Vec<KeyRow>, pub as_of_ms: u64, pub sources: Vec<Source>, pub narrowed: bool,
            pub can_ask: Vec<String>, pub call_id: String
        }
        pub enum CrError { Refused { needs: String }, Source { cell: String, why: String }, Invalid { why: String }, NotFound { what: String } }
    }
    ops {
        1 => HealthOp(()) -> Health, needs "cr.keys:health";
        2 => List(KeysAsk) -> Result<KeysView, CrError>, needs "cr.keys:read", deadline_ms 15000;
        3 => One(String) -> Result<KeyRow, CrError>, needs "cr.keys:read", deadline_ms 15000;
    }
}
