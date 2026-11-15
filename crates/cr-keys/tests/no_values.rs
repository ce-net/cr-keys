//! cr.keys@2, as `#[cell_test(sim, as = [operator, root])]` would run it: one body per viewer chain, a seeded store.
//! Secrets never leave the keys cell: this cell's own grant cannot open a value, a row carries none, a viewer without
//! `cr.keys:holders` gets rows with no holders on the wire, and row caveats narrow rows.
use ce_wire::Caller;
use cr_keys::*;
use cr_keys_api::*;
use cr_sources::keys::{GrantW, KeyInfo, MetaW};

const NOW: i64 = 1_800_000_000;
const SECRET: &str = "sk_live_THIS_IS_THE_VALUE_0123456789";

fn info(name: &str, service: &str, ends_in_days: Option<i64>) -> KeyInfo {
    KeyInfo {
        name: name.into(), version: 3, rotate: true, sha256_8: "a1b2c3d4".into(), written_by: "node-1".into(), ts: NOW - 40 * DAY,
        grants: ends_in_days.map(|d| vec![GrantW { home: "ab".repeat(32), name: "node-1".into(), lanes: vec!["*".into()], not_after: NOW + d * DAY }]).unwrap_or_default(),
        meta: MetaW { purpose: "payments".into(), service: service.into(), owner_lane: "billing".into(), given_by: "leif".into(), given_at: NOW - 40 * DAY, rotate_url: "https://dashboard.example/keys".into(), allowed_origins: vec![] },
        last_opened: NOW - 3600, last_check: "ok".into(), last_check_at: NOW - 600,
    }
}

/// What ce-gate hands a cell for a chain with these leaf abilities.
fn caller(abilities: &[&str]) -> Caller {
    let mut rights: Vec<String> = ["cr.keys:read", "cr.keys:health"].iter().filter(|r| abilities.contains(r)).map(|r| r.to_string()).collect();
    rights.extend(abilities.iter().map(|a| format!("ability:{a}")));
    Caller { name: "cd".repeat(32), rights }
}

fn all() -> Vec<KeyInfo> {
    vec![info("STRIPE_KEY", "stripe", Some(12)), info("GITHUB_TOKEN", "github", Some(200)), info("CONTROL_ROOM_DRY_RUN", "control-room", None)]
}

const ANY: KeysQuery = KeysQuery { q: None, due_within_days: None };

#[test]
fn the_cells_grant_has_no_keys_read() {
    assert!(SOURCE_ABILITIES.iter().all(|a| !a.contains("read") && !a.ends_with('*') && !a.contains("write")), "{SOURCE_ABILITIES:?}");
}

#[test]
fn no_value_bytes_in_any_key_row() {
    let mut seeded = all();
    // even a source row that somehow carried a value in a text field could not reach a KeyRow field named for it
    seeded[0].meta.rotate_url = SECRET.into();
    let rows = for_caller(&caller(&["cr.keys:read", "cr.keys:holders", "cr.keys:history"]), &ANY, &project(&seeded, NOW));
    let json = serde_json::to_string(&rows).unwrap();
    assert!(!json.contains(SECRET), "{json}");
    for f in ["value", "secret", "envelope", "plaintext", "rotate_url"] {
        assert!(!json.contains(&format!("\"{f}\"")), "a row field named {f}: {json}");
    }
    let postcard = ce_wire::postcard::to_stdvec(&rows).unwrap();
    assert!(!postcard.windows(SECRET.len()).any(|w| w == SECRET.as_bytes()));
}

#[test]
fn as_root_and_as_a_narrow_root_chain() {
    let p = project(&all(), NOW);
    let root = for_caller(&caller(&["cr.keys:read", "cr.keys:holders", "cr.keys:history"]), &ANY, &p);
    assert_eq!(root.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["STRIPE_KEY", "GITHUB_TOKEN", "CONTROL_ROOM_DRY_RUN"]);
    assert_eq!(root[0].rotation, RotationState::DueSoon { days: 12 });
    assert_eq!(root[0].holders.as_ref().map(|h| h.len()), Some(1));
    // the Part field is absent on the wire, not empty: postcard None is one zero byte where the Vec would be
    let narrow = for_caller(&caller(&["cr.keys:read"]), &ANY, &p);
    assert!(narrow.iter().all(|r| r.holders.is_none() && r.last_opened.is_none()));
    let wire = ce_wire::postcard::to_stdvec(&narrow).unwrap();
    assert!(!wire.windows(6).any(|w| w == b"node-1"), "no holder name on the wire");
}

#[test]
fn row_caveats_narrow_rows() {
    let p = project(&all(), NOW);
    let rows = for_caller(&caller(&["cr.keys:read", "cr.keys:where:service=git*|control-room"]), &ANY, &p);
    assert_eq!(rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["GITHUB_TOKEN", "CONTROL_ROOM_DRY_RUN"]);
    let v1 = for_caller(&caller(&["cr.keys:read", "cr.keys:scope:service=stripe"]), &ANY, &p);
    assert_eq!(v1.len(), 1, "the first edition's scope spelling is the same caveat");
    let due = KeysQuery { q: None, due_within_days: Some(30) };
    assert_eq!(for_caller(&caller(&["cr.keys:read"]), &due, &p).len(), 1);
}

#[test]
fn watch_deltas_carry_only_changes() {
    let c = caller(&["cr.keys:read", "cr.keys:holders"]);
    let before = for_caller(&c, &ANY, &project(&all(), NOW));
    assert!(delta(&before, &before, 1).is_none());
    let mut next = all();
    next.remove(1);
    next[0].version = 4;
    let after = for_caller(&c, &ANY, &project(&next, NOW));
    let d = delta(&before, &after, 2).unwrap();
    assert_eq!(d.put.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["STRIPE_KEY"]);
    assert_eq!(d.del, vec!["GITHUB_TOKEN".to_string()]);
}

#[test]
fn the_view_is_declared_with_the_right_it_needs() {
    let v = &VIEWS[0];
    assert_eq!((v.api, v.major, v.name, v.needs), ("cr.keys", 2, "KeyList", "cr.keys:read"));
    assert_eq!(v.streams, &[("rows", "Watch", "name")]);
    assert!(PARTS.iter().any(|p| p.field == "holders" && p.right == "cr.keys:holders"));
}
