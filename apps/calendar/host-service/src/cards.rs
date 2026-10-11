//! Durable app-owned projections. Restoring a card never creates an event
//! or sends another notification; edits reproject the saved event.
use super::*;

static CARDS: Mutex<()> = Mutex::new(());
pub type CardRetirer = Arc<dyn Fn(&str, &str) -> Result<(), String> + Send + Sync>;
static RETIRER: Mutex<Option<CardRetirer>> = Mutex::new(None);

pub fn on_withdraw_card(retirer: Option<CardRetirer>) {
    *RETIRER.lock().unwrap() = retirer;
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Publication {
    pub event: String,
    pub args: Value,
    pub published: u64,
    pub expires: u64,
    pub dismissed: bool,
}

fn read(root: &Path) -> Result<Vec<Publication>, String> {
    match std::fs::read(root.join("calendar/cards.json")) {
        Ok(bytes) => {
            serde_json::from_slice(&bytes).map_err(|e| format!("Cannot read Calendar cards: {e}"))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("Cannot read Calendar cards: {e}")),
    }
}
fn write(root: &Path, cards: &[Publication]) -> Result<(), String> {
    let dir = root.join("calendar");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let temp = dir.join(".cards.json.tmp");
    std::fs::write(&temp, serde_json::to_vec_pretty(cards).unwrap()).map_err(|e| e.to_string())?;
    std::fs::rename(temp, dir.join("cards.json")).map_err(|e| e.to_string())
}

pub fn publications(root: &Path, now: u64) -> Result<Vec<Publication>, String> {
    let _guard = CARDS.lock().unwrap();
    Ok(read(root)?
        .into_iter()
        .filter(|p| !p.dismissed && p.expires > now)
        .collect())
}

pub fn set_dismissed(root: &Path, card_id: &str, dismissed: bool) -> Result<(), String> {
    let _guard = CARDS.lock().unwrap();
    let mut cards = read(root)?;
    if let Some(card) = cards
        .iter_mut()
        .find(|p| text(&p.args, "card_id") == card_id)
    {
        card.dismissed = dismissed;
        write(root, &cards)?;
    }
    Ok(())
}

pub fn publish(
    root: &Path,
    event: &Event,
    card_id: &str,
    priority: i64,
    now: u64,
    publisher: &CardPublisher,
) -> Result<Value, String> {
    let args = saved_event_card_args(event, card_id, priority);
    {
        let _guard = CARDS.lock().unwrap();
        if let Some(old) = read(root)?
            .iter()
            .find(|p| !p.dismissed && p.expires > now && p.args == args)
        {
            return Ok(
                json!({"card_id":card_id,"replaced":true,"expires_at":old.expires,"reused":true}),
            );
        }
    }
    // Never hold the projection file lock while calling the shell.
    let result = publisher(APP, args.clone())?;
    let _guard = CARDS.lock().unwrap();
    let mut cards = read(root)?;
    cards.retain(|p| p.expires > now && text(&p.args, "card_id") != card_id);
    cards.push(Publication {
        event: event.id.clone(),
        args,
        published: now,
        expires: result["expires_at"].as_u64().unwrap_or(now + 86_400_000),
        dismissed: false,
    });
    write(root, &cards)?;
    Ok(result)
}

pub fn refresh(
    root: &Path,
    event: &Event,
    publisher: Option<&CardPublisher>,
) -> Result<(), String> {
    let now = chrono::Utc::now().timestamp_millis().max(0) as u64;
    let cards = publications(root, now)?;
    for old in cards.into_iter().filter(|p| p.event == event.id) {
        let Some(publisher) = publisher else {
            return Err("Calendar saved the event, but Glance is unavailable.".into());
        };
        let mut args = saved_event_card_args(
            event,
            text(&old.args, "card_id"),
            old.args["priority"].as_i64().unwrap_or(70),
        );
        args["notify"] = json!(false);
        args["expires"] = json!((old.expires.saturating_sub(now) / 1000).clamp(60, 604800));
        publisher(APP, args.clone())?;
        let _guard = CARDS.lock().unwrap();
        let mut all = read(root)?;
        if let Some(p) = all
            .iter_mut()
            .find(|p| text(&p.args, "card_id") == text(&old.args, "card_id"))
        {
            // Keep the original notification identity and deadline.
            p.args = saved_event_card_args(
                event,
                text(&old.args, "card_id"),
                old.args["priority"].as_i64().unwrap_or(70),
            );
        }
        write(root, &all)?;
    }
    Ok(())
}

pub fn remove(root: &Path, event: &str) -> Result<(), String> {
    let retiring = {
        let _guard = CARDS.lock().unwrap();
        let mut all = read(root)?;
        let retiring: Vec<_> = all
            .iter()
            .filter(|p| p.event == event)
            .map(|p| text(&p.args, "card_id").to_string())
            .collect();
        all.retain(|p| p.event != event);
        write(root, &all)?;
        retiring
    };
    let retirer = RETIRER.lock().unwrap().clone();
    if let Some(retirer) = retirer {
        for id in retiring {
            retirer(APP, &id)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retry_edit_dismiss_restart_and_delete_use_one_saved_event() {
        let root = std::env::temp_dir().join(format!("calendar-projection-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let calls: Arc<Mutex<Vec<Value>>> = Arc::default();
        let received = calls.clone();
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let publisher: CardPublisher = Arc::new(move |app, args| {
            assert_eq!(app, APP);
            let result = json!({"card_id":args["card_id"],"expires_at":now+86_400_000});
            received.lock().unwrap().push(args);
            Ok(result)
        });
        let at = Local::now().naive_local();
        let added = handle(APP,"add_event",&json!({"title":"Fixture delivery","start":"2026-10-06T09:00","timezone":"America/Los_Angeles"}),&root,at,None).unwrap();
        let event = load(&root).remove(0);
        let args = json!({"event":added["id"]});
        handle(APP, "notify", &args, &root, at, Some(&publisher)).unwrap();
        assert_eq!(
            handle(APP, "notify", &args, &root, at, Some(&publisher)).unwrap()["reused"],
            true
        );
        assert_eq!(
            calls.lock().unwrap().len(),
            1,
            "retry must not send another banner"
        );
        let mut edit = serde_json::to_value(&event).unwrap();
        edit["expected"] = serde_json::to_value(&event).unwrap();
        edit["start"] = json!("2026-10-06T10:30");
        handle(APP, "update_event", &edit, &root, at, Some(&publisher)).unwrap();
        let seen = calls.lock().unwrap();
        assert_eq!(seen[1]["data"]["ev"]["metric2_value"], "10:30 PDT");
        assert_eq!(seen[1]["notify"], false);
        assert_eq!(seen[1]["open"]["route"], format!("event/{}", event.id));
        drop(seen);
        let restored = publications(&root, now + 1000).unwrap();
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].args["data"]["ev"]["metric2_value"], "10:30 PDT");
        set_dismissed(&root, &event.id, true).unwrap();
        assert!(publications(&root, now + 1000).unwrap().is_empty());
        set_dismissed(&root, &event.id, false).unwrap();
        assert_eq!(publications(&root, now + 1000).unwrap().len(), 1);
        handle(
            APP,
            "remove_event",
            &json!({"id":event.id}),
            &root,
            at,
            None,
        )
        .unwrap();
        assert!(load(&root).is_empty() && publications(&root, now + 1000).unwrap().is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }
}
