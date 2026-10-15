//! Real journal queries over synthetic history; never a network benchmark.
use action_receipts::{Action, Authority, Grant, Journal};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::{hint::black_box, time::Instant};

fn measure(journal: &Journal, grant: &Grant) -> Value {
    let mut times = Vec::new();
    for _ in 0..50 {
        let started = Instant::now();
        assert_eq!(black_box(journal.pending(grant, 101).unwrap()).len(), 3);
        assert_eq!(black_box(journal.recent(grant, 101).unwrap()).len(), 40);
        times.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(f64::total_cmp);
    json!({"samples":times.len(), "p50_ms":times[24], "p95_ms":times[47], "maximum_ms":times[49]})
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let folder = std::env::temp_dir().join(format!("buwei-benchmark-{}", action_receipts::new_id()));
    std::fs::create_dir(&folder)?;
    let path = folder.join("fixture.db");
    let authority = Authority::default();
    authority.set_account(Some("fixture-owner"));
    let grant = authority.grant("fixture-app", &["read"], 100, 3600)?;
    let mut journal = Journal::open(&path)?;
    let template = journal.prepare(&grant, Action { permission:"read".into(), target:"fixture-target".into(), summary:"synthetic benchmark".into(), payload:json!({}) }, 0, 100, 120)?;
    let mut db = Connection::open(&path)?;
    let raw: String = db.query_row("SELECT body FROM operations WHERE id=?1", [&template.id], |row| row.get(0))?;
    // This temporary fixture starts with the previously shipped table schema.
    // The baseline uses the same production query functions, grant and rows.
    db.execute_batch("DROP INDEX operations_scope; DROP INDEX operations_pending_scope; DELETE FROM operations;")?;
    let transaction = db.transaction()?;
    {
        let mut insert = transaction.prepare("INSERT INTO operations(id,app,account,status,body) VALUES(?1,?2,?3,?4,?5)")?;
        for n in 0..50_008 {
            let mut body: Value = serde_json::from_str(&raw)?;
            let id = format!("{n:032x}");
            let account = "fixture-owner";
            let status = if n < 3 { "unknown" } else { "confirmed" };
            body["id"] = id.clone().into(); body["account"] = account.into(); body["status"] = status.into();
            body["receipt"] = if n < 3 {
                json!({"status":"unknown","evidence":null,"message":"synthetic acknowledgement missing"})
            } else {
                json!({"status":"confirmed","message":"synthetic historical result","evidence":{"operation_id":id,"external_id":format!("fixture:{id}"),"account":account,"target":body["action"]["target"],"digest":body["digest"]}})
            };
            insert.execute(params![id, "fixture-app", account, status, body.to_string()])?;
        }
    }
    transaction.commit()?;
    let ids = |j: &Journal| j.recent(&grant, 101).unwrap().into_iter().map(|op| op.id).collect::<Vec<_>>();
    let original = ids(&journal);
    let before = measure(&journal, &grant);
    drop(journal);
    let journal = Journal::open(&path)?; // The actual backwards-compatible upgrade.
    assert_eq!(ids(&journal), original);
    let after = measure(&journal, &grant);
    println!("{}", json!({"schema":1,"passed":true,"kind":"synthetic_single_account_history","rows":50008,"owned_rows":50008,"pending_rows":3,"results_equal":true,"network_calls":0,"before":before,"after":after}));
    drop(journal); drop(db);
    // Only this program's random, explicitly created temporary fixture.
    std::fs::remove_file(&path)?;
    for suffix in ["-wal", "-shm"] { let _ = std::fs::remove_file(folder.join(format!("fixture.db{suffix}"))); }
    std::fs::remove_dir(&folder)?;
    Ok(())
}
