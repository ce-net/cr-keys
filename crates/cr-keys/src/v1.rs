//! Major 1 (frozen contract `cr_keys_api::versions::v1`): the first edition's envelope, computed from the same
//! `keys` rows as major 2. Unchanged behaviour; only the chain reader moved to ui-chain.
use crate::{CELL, DAY, WINDOW_DAYS};
use cr_keys_api::versions::v1::*;
use cr_sources::keys::{GrantW, KeyInfo};
use ui_chain as cr_view;

pub fn rotation(i: &KeyInfo, now: i64) -> (RotationState, Option<i64>) {
    // the first share that is still live ends the soonest; a key whose every share ended is overdue since the last one
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

pub fn row(i: &KeyInfo, now: i64, with_holders: bool) -> KeyRow {
    let (rotation, expires_at) = rotation(i, now);
    KeyRow {
        name: i.name.clone(),
        service: i.meta.service.clone(),
        purpose: i.meta.purpose.clone(),
        owner: i.meta.owner_lane.clone(),
        version: i.version,
        sha256_8: i.sha256_8.clone(),
        written_by: i.written_by.clone(),
        written_at: i.ts,
        expires_at,
        days_left: expires_at.map(|t| (t - now).div_euclid(DAY)),
        rotation,
        self_renewing: i.rotate,
        holders: if with_holders { i.grants.iter().map(holder).collect() } else { vec![] },
        holder_count: if with_holders { i.grants.len() as u32 } else { 0 },
        last_opened: i.last_opened,
        last_check: i.last_check.clone(),
        last_check_at: i.last_check_at,
        refs: vec!["cell:keys".into(), format!("key:{}", i.name)],
    }
}

/// The rows this caller may see of `infos`, and whether the chain removed any row or field.
pub fn view(c: &ce_wire::Caller, ask: &KeysAsk, infos: &[KeyInfo], now: i64) -> (Vec<KeyRow>, bool, Vec<String>) {
    let services = cr_view::scopes(c, CELL, "service");
    let owners = cr_view::scopes(c, CELL, "owner");
    let with_holders = cr_view::holds(c, "cr.keys:holders");
    let q = ask.q.to_lowercase();
    let mut narrowed = !with_holders && infos.iter().any(|i| !i.grants.is_empty());
    let mut rows = Vec::new();
    for i in infos {
        if !cr_view::in_scope(&services, &i.meta.service) || !cr_view::in_scope(&owners, &i.meta.owner_lane) {
            narrowed = true;
            continue;
        }
        let r = row(i, now, with_holders);
        let hit = q.is_empty() || [&r.name, &r.service, &r.purpose].iter().any(|s| s.to_lowercase().contains(&q));
        let due = ask.due_within_days.is_none_or(|d| r.days_left.is_some_and(|l| l <= d as i64));
        if hit && due {
            rows.push(r);
        }
    }
    rows.sort_by(|a, b| a.days_left.unwrap_or(i64::MAX).cmp(&b.days_left.unwrap_or(i64::MAX)).then(a.name.cmp(&b.name)));
    let mut can_ask = Vec::new();
    if !with_holders {
        can_ask.push("cr.keys:holders".into());
    }
    if !services.is_empty() || !owners.is_empty() {
        can_ask.push("cr.keys:read (unscoped)".into());
    }
    (rows, narrowed, can_ask)
}
