//! Major 1, frozen: the first edition's tests, unchanged, against the v1 module.
use ce_wire::Caller;
use cr_keys::v1::*;
use cr_keys::{DAY, SOURCE_ABILITIES};
use cr_keys_api::versions::v1::*;
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

fn caller(abilities: &[&str]) -> Caller {
    let mut rights = vec!["cr.keys:read".to_string()];
    rights.extend(abilities.iter().map(|a| format!("ability:{a}")));
    Caller { name: "cd".repeat(32), rights }
}

#[test]
fn the_cells_grant_has_no_keys_read() {
    assert!(SOURCE_ABILITIES.iter().all(|a| !a.contains("read") && !a.ends_with('*') && !a.contains("write")), "{SOURCE_ABILITIES:?}");
}

#[test]
fn row_json_holds_no_value() {
    // the value lives only in the sealed envelope in the keys cell; metadata is all KeyInfo has and all KeyRow takes
    let rows = view(&caller(&["cr.keys:read", "cr.keys:holders"]), &KeysAsk { q: String::new(), due_within_days: None }, &[info("STRIPE_KEY", "stripe", Some(12))], NOW).0;
    let json = serde_json::to_string(&rows).unwrap();
    assert!(!json.contains(SECRET));
    for f in ["value", "secret", "envelope", "plaintext"] {
        assert!(!json.contains(&format!("\"{f}\"")), "a row field named {f}: {json}");
    }
}

#[test]
fn rotation_states_and_days_left() {
    let st = |d: Option<i64>| rotation(&info("k", "s", d), NOW).0;
    assert_eq!(st(Some(90)), RotationState::Fresh);
    assert_eq!(st(Some(12)), RotationState::DueSoon { days: 12 });
    assert_eq!(st(Some(-3)), RotationState::Overdue { days: 3 });
    assert_eq!(st(None), RotationState::Manual);
    let mut f = info("k", "s", Some(90));
    f.last_check = "fail: 401 from the provider".into();
    assert_eq!(rotation(&f, NOW).0, RotationState::Failed { why: "401 from the provider".into() });
}

#[test]
fn holders_need_their_own_right_and_scopes_narrow_rows() {
    let all = [info("STRIPE_KEY", "stripe", Some(12)), info("GITHUB_TOKEN", "github", Some(200))];
    let ask = KeysAsk { q: String::new(), due_within_days: None };
    let (rows, narrowed, can_ask) = view(&caller(&["cr.keys:read", "cr.keys:holders"]), &ask, &all, NOW);
    assert_eq!(rows.len(), 2);
    assert!(!narrowed && can_ask.is_empty());
    assert_eq!(rows[0].name, "STRIPE_KEY", "soonest expiry first");
    assert_eq!(rows[0].holders.len(), 1);

    let (rows, narrowed, can_ask) = view(&caller(&["cr.keys:read"]), &ask, &all, NOW);
    assert!(narrowed && rows.iter().all(|r| r.holders.is_empty() && r.holder_count == 0));
    assert_eq!(can_ask, vec!["cr.keys:holders".to_string()]);

    let (rows, narrowed, _) = view(&caller(&["cr.keys:read", "cr.keys:holders", "cr.keys:scope:service=git*"]), &ask, &all, NOW);
    assert!(narrowed);
    assert_eq!(rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["GITHUB_TOKEN"]);

    let due = KeysAsk { q: String::new(), due_within_days: Some(30) };
    assert_eq!(view(&caller(&["cr.keys:read", "cr.keys:holders"]), &due, &all, NOW).0.len(), 1);
}

#[test]
fn major_1_is_frozen_byte_for_byte() {
    use ce_wire::Api;
    assert_eq!(<cr_keys_api::versions::v1::CrKeys as Api>::SCHEMA, 0xf4f284ea9d9f783e, "the schema the first edition's clients speak");
    assert_eq!(<cr_keys_api::CrKeys as Api>::VERSION, 2);
}
