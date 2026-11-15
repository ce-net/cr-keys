//! `cr.keys@2`: the Keys panel's full-stack cell. Key metadata only: name, service, holders, expiry, rotation state,
//! version, short hash and last check. `KeyRow` has no field that can carry a key's value, and the cell's own grant on
//! `keys` has no `keys:read`, so a value can neither be asked for nor returned.
//!
//! Rights: `cr.keys:read` for `List`, `Watch` and `Sources` and for the `KeyList` view (no manifest entry, no bundle
//! without it). Field level: `holders` needs `cr.keys:holders`, `last_opened` needs `cr.keys:history` (absent on the
//! wire otherwise). Row level: `cr.keys:where:service=..` and `cr.keys:where:owner=..` caveats in the chain.
//! Major 1 (`versions/v1.rs`, the first edition's envelope) is frozen and still served by the same backend.
#![forbid(unsafe_code)]

pub mod versions {
    pub mod v1;
}

ce_wire::api! {
    api CrKeys = "cr.keys", version 2;
    types {
        /// Health contract: these three fields, in this order, in every cell.
        pub struct Health { pub ok: bool, pub version: String, pub uptime_s: u64 }
        /// How a source cell answered: live, stale (age of the copy shown), down (why), or refused (the right this
        /// cell lacks on it).
        pub enum SourceState { Live, Stale { age_s: u64 }, Down { why: String }, Refused { needs: String } }
        pub struct Source { pub cell: String, pub node: String, pub state: SourceState, pub ms: u32 }
        /// Fresh: every share lasts more than the renewal window. DueSoon/Overdue: the first share ends in / ended
        /// `days` ago. Rotating: a renewal is running. Failed: the last key check failed. Manual: no share ends.
        pub enum RotationState { Fresh, DueSoon { days: u32 }, Overdue { days: u32 }, Rotating, Failed { why: String }, Manual }
        /// A home holding a share of the key: its name, node id (hex), the lanes it may open it for, when the share ends.
        pub struct Holder { pub name: String, pub home: String, pub lanes: Vec<String>, pub not_after: i64 }
        /// `holders` is a Part (cr.keys:holders), `last_opened` a Part (cr.keys:history): None on the wire for a
        /// viewer without that right.
        pub struct KeyRow {
            pub name: String, pub service: String, pub purpose: String, pub owner: String,
            pub version: i64, pub sha256_8: String, pub written_at: i64,
            pub expires_at: Option<i64>, pub days_left: Option<i64>, pub rotation: RotationState, pub self_renewing: bool,
            pub last_check: String, pub last_check_at: i64,
            pub holders: Option<Vec<Holder>>, pub last_opened: Option<i64>,
            pub refs: Vec<String>
        }
        /// q: substring of name, service or purpose. due_within_days: only keys whose first share ends within that.
        pub struct KeysQuery { pub q: Option<String>, pub due_within_days: Option<u32> }
        /// One item of `Watch`: reset = replace all rows; else put upserts (by name) and del removes.
        pub struct KeyDelta { pub reset: bool, pub put: Vec<KeyRow>, pub del: Vec<String>, pub hlc: u64 }
    }
    ops {
        1 => HealthOp(()) -> Health, needs "cr.keys:health";
        2 => List(KeysQuery) -> Vec<KeyRow>, needs "cr.keys:read", deadline_ms 15000;
        6 => Sources(()) -> Vec<Source>, needs "cr.keys:read", deadline_ms 15000;
    }
    streams {
        3 => Watch(KeysQuery) -> stream KeyDelta, needs "cr.keys:read";
    }
}

ui_part::views! {
    api CrKeys;
    view KeyList {
        props { q: Option<String>, due_within_days: Option<u32> }
        needs "cr.keys:read";
        events { opened(String); renew(String); rotate(String) }
        data { sources: Sources }
        streams { rows: Watch by name }
        slots {}
        speaks [2];
    }
    parts { KeyRow { holders: "cr.keys:holders", last_opened: "cr.keys:history" } }
}
