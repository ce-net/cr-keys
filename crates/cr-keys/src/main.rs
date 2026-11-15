//! `cr-keys serve`: the cr.keys cell behind this machine's door (ce-cap, default deny), majors 2 and 1 side by side.
//! A background task rebuilds the projection from `keys` `List` (this node's keys cell first, then any replica) with
//! [`cr_keys::SOURCE_ABILITIES`] only, every [`cr_keys::EVERY_S`] s; every op and every `Watch` reads that projection.
use ce_wire::Server;
use cr_keys::*;
use cr_keys_api::*;
use cr_sources::keys::{self as k, KeyInfo, Keys};
use cr_sources::trace::Trace;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{watch, RwLock};

#[derive(Clone, Default)]
struct Projection {
    infos: Vec<KeyInfo>,
    rows: BTreeMap<String, KeyRow>,
    source: Option<Source>,
    at: Option<Instant>,
    hlc: u64,
}

type Shared = Arc<RwLock<Projection>>;

fn node() -> String {
    ui_chain::hostname()
}

async fn fetch(t: &Trace) -> Result<Vec<KeyInfo>, String> {
    let span = t.start("keys", "List", &node());
    let got = async {
        let tok = ui_chain::source_token::<Keys>(SOURCE_ABILITIES, 120, None)?;
        let mut c = ce_wire::Client::<Keys>::by_name(tok).await.map_err(|e| e.to_string())?;
        c.deadline = Duration::from_secs(10);
        c.call::<k::List>(&()).await.map_err(|e| e.to_string())
    }
    .await;
    t.end(span, got.as_ref().map(|_| "ok").unwrap_or_else(|e| e.as_str()));
    got
}

async fn refresh(p: &Shared, tx: &watch::Sender<u64>) {
    let start = ui_chain::now_ms();
    let t = Trace::root(&node());
    let t0 = Instant::now();
    let got = fetch(&t).await;
    let ms = t0.elapsed().as_millis() as u32;
    let mut g = p.write().await;
    match got {
        Ok(infos) => {
            let rows = project(&infos, ui_chain::now_s());
            let changed = rows != g.rows;
            g.infos = infos;
            g.rows = rows;
            g.at = Some(Instant::now());
            g.source = Some(Source { cell: "keys".into(), node: node(), state: SourceState::Live, ms });
            if changed {
                g.hlc = ui_chain::now_ms();
                let _ = tx.send(g.hlc);
            }
            t.finish(CELL, "Project", start, "ok");
        }
        Err(why) => {
            // the last good copy stays, marked stale; never an empty table for a source that blinked
            let state = match g.at {
                Some(at) => SourceState::Stale { age_s: at.elapsed().as_secs() },
                None if why.contains("Right") => SourceState::Refused { needs: "keys:list".into() },
                None => SourceState::Down { why: why.clone() },
            };
            g.source = Some(Source { cell: "keys".into(), node: node(), state, ms });
            t.finish(CELL, "Project", start, &why);
        }
    }
}

fn sources(g: &Projection) -> Vec<Source> {
    let mut s = g.source.clone().unwrap_or(Source { cell: "keys".into(), node: node(), state: SourceState::Down { why: "not read yet".into() }, ms: 0 });
    if let (SourceState::Live, Some(at)) = (&s.state, g.at) {
        if at.elapsed().as_secs() > 2 * EVERY_S {
            s.state = SourceState::Stale { age_s: at.elapsed().as_secs() };
        }
    }
    vec![s]
}

async fn serve() -> Result<(), String> {
    // this cell's own key and narrow grant (never the node's key); the gate roots callers' chains at the grant's root
    let me = ui_chain::init(CELL)?.root();
    let started = Instant::now();
    let p: Shared = Arc::new(RwLock::new(Projection::default()));
    let (tx, rx) = watch::channel(0u64);
    {
        let p = p.clone();
        tokio::spawn(async move {
            loop {
                refresh(&p, &tx).await;
                tokio::time::sleep(Duration::from_secs(EVERY_S)).await;
            }
        });
    }
    let (p2, p3, p4, p1) = (p.clone(), p.clone(), p.clone(), p.clone());
    let gate = ce_gate::Gate::<CrKeys>::new(me, ce_gate::mesh_roots()).into_fn();
    let v2 = Server::<CrKeys>::new(gate)
        .on::<HealthOp, _, _>(move |_: (), _| async move { Ok(Health { ok: true, version: env!("CARGO_PKG_VERSION").into(), uptime_s: started.elapsed().as_secs() }) })
        .on::<List, _, _>(move |q: KeysQuery, who| {
            let p = p2.clone();
            async move {
                let t = Trace::root(&node());
                let start = ui_chain::now_ms();
                let rows = for_caller(&who, &q, &p.read().await.rows);
                t.finish(CELL, &format!("List chain={}", ui_chain::chain_id(&who)), start, "ok");
                Ok(rows)
            }
        })
        .on::<Sources, _, _>(move |_: (), who| {
            let p = p3.clone();
            async move { Ok(sources_for(&who, sources(&*p.read().await))) }
        })
        .stream::<Watch, _, _>(move |q: KeysQuery, who, sink| {
            let (p, mut rx) = (p4.clone(), rx.clone());
            async move {
                let end = Instant::now() + Duration::from_secs(STREAM_MAX_S);
                let (mut sent, hlc) = {
                    let g = p.read().await;
                    (for_caller(&who, &q, &g.rows), g.hlc)
                };
                sink.send(&KeyDelta { reset: true, put: sent.clone(), del: vec![], hlc }).map_err(|e| e.to_string())?;
                loop {
                    let left = end.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Ok(());
                    }
                    match tokio::time::timeout(left, rx.changed()).await {
                        Err(_) => return Ok(()),
                        Ok(Err(_)) => return Ok(()),
                        Ok(Ok(())) => {}
                    }
                    let g = p.read().await;
                    let now = for_caller(&who, &q, &g.rows);
                    if let Some(d) = delta(&sent, &now, g.hlc) {
                        sink.send(&d).map_err(|e| e.to_string())?;
                        sent = now;
                    }
                }
            }
        })
        .audit(ce_gate::audit::file())
        .serve_door();

    // major 1, frozen: the first edition's envelope from the same projection
    use cr_keys_api::versions::v1 as old;
    let gate1 = ce_gate::Gate::<old::CrKeys>::new(me, ce_gate::mesh_roots()).into_fn();
    let p5 = p1.clone();
    let v1 = Server::<old::CrKeys>::new(gate1)
        .on::<old::HealthOp, _, _>(move |_: (), _| async move { Ok(old::Health { ok: true, version: env!("CARGO_PKG_VERSION").into(), uptime_s: started.elapsed().as_secs() }) })
        .on::<old::List, _, _>(move |ask: old::KeysAsk, who| {
            let p = p1.clone();
            async move { Ok(Ok(list_v1(&*p.read().await, &ask, &who))) }
        })
        .on::<old::One, _, _>(move |name: String, who| {
            let p = p5.clone();
            async move {
                let v = list_v1(&*p.read().await, &old::KeysAsk { q: String::new(), due_within_days: None }, &who);
                Ok(v.rows.into_iter().find(|r| r.name == name).ok_or(old::CrError::NotFound { what: name }))
            }
        })
        .audit(ce_gate::audit::file())
        .serve_door();
    let (a, b) = tokio::join!(v2, v1);
    a.map_err(|e| e.to_string())?;
    b.map_err(|e| e.to_string())
}

fn list_v1(g: &Projection, ask: &cr_keys_api::versions::v1::KeysAsk, who: &ce_wire::Caller) -> cr_keys_api::versions::v1::KeysView {
    use cr_keys_api::versions::v1 as old;
    let (rows, narrowed, can_ask) = v1::view(who, ask, &g.infos, ui_chain::now_s());
    let src = sources(g).into_iter().next().map(|s| old::Source {
        cell: s.cell,
        node: s.node,
        ms: s.ms,
        state: match s.state {
            SourceState::Live => old::SourceState::Live,
            SourceState::Stale { age_s } => old::SourceState::Stale { age_s },
            SourceState::Down { why } => old::SourceState::Down { why },
            SourceState::Refused { needs } => old::SourceState::Refused { needs },
        },
    });
    old::KeysView { rows, as_of_ms: ui_chain::now_ms(), sources: src.into_iter().collect(), narrowed, can_ask, call_id: String::new() }
}

fn main() {
    let r = match std::env::args().nth(1).as_deref() {
        Some("serve") => tokio::runtime::Builder::new_multi_thread().enable_all().build().map_err(|e| e.to_string()).and_then(|rt| rt.block_on(serve())),
        _ => Err("usage: cr-keys serve".into()),
    };
    if let Err(e) = r {
        eprintln!("cr-keys: {e}");
        std::process::exit(1);
    }
}
