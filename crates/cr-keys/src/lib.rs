//! The one function of `cr.keys`: rows of key metadata for the Keys panel, filtered by the caller's chain.
//!
//! The cell reads `keys` `List` with a grant of exactly [`SOURCE_ABILITIES`]. `keys:read` (which opens a value) is not
//! among them, and `KeyRow` has no field a value could travel in, so no viewer, however wide their chain, can get a
//! secret through this cell. The rows are a projection of `keys` (a cache, rebuilt from it every [`EVERY_S`] s); what a
//! viewer gets of it is decided here, per call and per `Watch` delta: row caveats (`layer_view`), then `Part` fields.
#![forbid(unsafe_code)]

pub mod v1;

use cr_keys_api::*;
use cr_sources::keys::{GrantW, KeyInfo};
use std::collections::BTreeMap;
use ui_chain::{layer_view, Caveats};
use ui_part::Parts;

/// What this cell may do on `keys`. Never `keys:read`, `keys:write` or `keys:*` (tests/no_values.rs).
pub const SOURCE_ABILITIES: &[&str] = &["keys:list", "keys:meta"];
/// The one grant this cell's own key holds, minted by the node for exactly what it reads (`ce-iam grant --to <cell key>
/// --action <each>`, kept with the key in `<ce data dir>/cell-keys/cr.keys/`); every source token is attenuated from it
/// ([`ui_chain::source_token`]). A taken backend holds this, never the node's root.
pub const GRANT: &[&str] = &["keys:list", "keys:meta", "ui.trace:write", "delegate:1"];
/// The renewal window of the keys autopilot (repo:keys-cell crates/keys-client/src/autopilot.rs `share_days`).
pub const WINDOW_DAYS: i64 = 30;
pub const DAY: i64 = 86_400;
pub const CELL: &str = "cr.keys";
/// How often the projection is rebuilt from `keys`.
pub const EVERY_S: u64 = 10;
/// A `Watch` stream ends after this; the page opens a new one with a new Hello, so an ended or revoked grant is
/// refused within this time (ce-gate decides rights once per connection).
pub const STREAM_MAX_S: u64 = 30;

pub fn rotation(i: &KeyInfo, now: i64) -> (RotationState, Option<i64>) {
    let live = i.grants.iter().map(|g| g.not_after).filter(|t| *t > now).min();
    let last = i.grants.iter().map(|g| g.not_after).max();
    let end = live.or(last);
    if let Some(why) = i.last_check.strip_prefix("fail") {
        return (RotationState::Failed { why: why.trim_start_matches(':').trim().to_string() }, end);
    }
    let st = match (live, last) {
        (None, None) => RotationState::Manual,
        (None, Some(t)) => RotationState::Overdue { days: ((now - t).max(0) / DAY) as u32 },
        (Some(t), _) if t - now < WINDOW_DAYS * DAY => RotationState::DueSoon { days: ((t - now) / DAY) as u32 },
        (Some(_), _) => RotationState::Fresh,
    };
    (st, end)
}

fn holder(g: &GrantW) -> Holder {
    Holder { name: g.name.clone(), home: g.home.clone(), lanes: g.lanes.clone(), not_after: g.not_after }
}

/// The projection row of one key, whole (every Part filled); [`for_caller`] withholds per viewer.
pub fn row(i: &KeyInfo, now: i64) -> KeyRow {
    let (rotation, expires_at) = rotation(i, now);
    KeyRow {
        name: i.name.clone(),
        service: i.meta.service.clone(),
        purpose: i.meta.purpose.clone(),
        owner: i.meta.owner_lane.clone(),
        version: i.version,
        sha256_8: i.sha256_8.clone(),
        written_at: i.ts,
        expires_at,
        days_left: expires_at.map(|t| (t - now).div_euclid(DAY)),
        rotation,
        self_renewing: i.rotate,
        last_check: i.last_check.clone(),
        last_check_at: i.last_check_at,
        holders: Some(i.grants.iter().map(holder).collect()),
        last_opened: Some(i.last_opened),
        refs: vec!["cell:keys".into(), format!("key:{}", i.name)],
    }
}

/// The projection: every key's row by name.
pub fn project(infos: &[KeyInfo], now: i64) -> BTreeMap<String, KeyRow> {
    infos.iter().map(|i| (i.name.clone(), row(i, now))).collect()
}

fn caveat(r: &KeyRow, k: &str) -> Option<String> {
    match k {
        "service" => Some(r.service.clone()),
        "owner" => Some(r.owner.clone()),
        "name" => Some(r.name.clone()),
        _ => None,
    }
}

fn matches(r: &KeyRow, q: &KeysQuery) -> bool {
    let text = q.q.as_deref().unwrap_or("").to_lowercase();
    let hit = text.is_empty() || [&r.name, &r.service, &r.purpose].iter().any(|s| s.to_lowercase().contains(&text));
    let due = q.due_within_days.is_none_or(|d| r.days_left.is_some_and(|l| l <= d as i64));
    hit && due
}

/// What one caller gets of the projection: the rows their caveats and query let through, each with the Part fields
/// their chain holds. Soonest to end first.
pub fn for_caller(c: &ce_wire::Caller, q: &KeysQuery, rows: &BTreeMap<String, KeyRow>) -> Vec<KeyRow> {
    let cv = Caveats::of(c, CELL);
    let mut out: Vec<KeyRow> = rows
        .values()
        .filter(|r| layer_view::visible_by(&cv, |k| caveat(r, k)) && matches(r, q))
        .map(|r| {
            let mut r = r.clone();
            r.withhold(&|right| ui_chain::holds(c, right));
            r
        })
        .collect();
    out.sort_by(|a, b| a.days_left.unwrap_or(i64::MAX).cmp(&b.days_left.unwrap_or(i64::MAX)).then(a.name.cmp(&b.name)));
    out
}

/// The delta from what a subscriber last got to what they get now (by name); None when nothing changed.
pub fn delta(before: &[KeyRow], now: &[KeyRow], hlc: u64) -> Option<KeyDelta> {
    let old: BTreeMap<&str, &KeyRow> = before.iter().map(|r| (r.name.as_str(), r)).collect();
    let new: BTreeMap<&str, &KeyRow> = now.iter().map(|r| (r.name.as_str(), r)).collect();
    let put: Vec<KeyRow> = now.iter().filter(|r| old.get(r.name.as_str()) != Some(r)).cloned().collect();
    let del: Vec<String> = before.iter().filter(|r| !new.contains_key(r.name.as_str())).map(|r| r.name.clone()).collect();
    (!put.is_empty() || !del.is_empty()).then_some(KeyDelta { reset: false, put, del, hlc })
}

/// The source lines this caller may see: filtered by the same `node` caveat as the rows (a mesh-wide line, with no
/// node, is not shown to a caller caveated on nodes), so a narrow viewer does not learn other nodes' names.
pub fn sources_for(c: &ce_wire::Caller, all: Vec<Source>) -> Vec<Source> {
    let cv = Caveats::of(c, CELL);
    all.into_iter().filter(|s| cv.0.get("node").is_none() || (!s.node.is_empty() && cv.allows("node", &s.node))).collect()
}
