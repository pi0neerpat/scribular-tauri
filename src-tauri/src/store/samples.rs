use rusqlite::{params, Transaction};

use super::{settings, Result, Store};

const SAMPLE_TRANSCRIPT: &str = "Therapist: How have things been since our last session? Client: It's been pretty stressful actually. I've been working a lot of overtime and I feel like I can't keep up. Therapist: That sounds really challenging. Can you tell me more about what's been happening? Client: Well, my boss keeps giving me more projects and I don't know how to say no. I'm worried about my job if I push back. Therapist: I understand that concern. Let's talk about some ways you might approach this conversation...";
const SAMPLE_SUMMARY: &str = "Client discussed ongoing challenges with work-life balance and expressed feeling overwhelmed by recent deadlines. Explored coping strategies including time management techniques and boundary setting. Client identified a need to communicate more effectively with their supervisor about workload capacity. Client set the goal to implement a daily planning routine and schedule a conversation with their supervisor within the next week.";
const SAMPLE_DURATION_MS: u32 = 1_245_000;
const DAY_MS: i64 = 86_400_000;

fn insert_sample(tx: &Transaction<'_>) -> Result<String> {
    let id = tx.query_row(
        "INSERT INTO sessions (id, title, created_at, input_mode, status, transcript, summary, duration_ms, is_sample)
         VALUES (lower(hex(randomblob(16))), 'Sample session', ?1, 'mic', 'done', ?2, ?3, ?4, 1)
         RETURNING id",
        params![super::now_ms() - DAY_MS, SAMPLE_TRANSCRIPT, SAMPLE_SUMMARY, SAMPLE_DURATION_MS],
        |r| r.get(0),
    )?;
    Ok(id)
}

impl Store {
    /// Seeds the single example session once, after the encrypted store opens.
    pub fn seed_samples_if_needed(&self) -> Result<()> {
        self.with(|c| {
            let tx = c.transaction()?;
            if settings::select(&tx, settings::SAMPLES_SEEDED)?.as_deref() == Some("1") {
                return Ok(());
            }
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM sessions WHERE is_sample = 1)",
                [],
                |r| r.get(0),
            )?;
            if !exists {
                insert_sample(&tx)?;
            }
            settings::insert(&tx, settings::SAMPLES_SEEDED, "1")?;
            tx.commit()?;
            Ok(())
        })
    }

    /// Replaces all example rows and returns the new sample id.
    pub fn restore_samples(&self) -> Result<String> {
        self.with(|c| {
            let tx = c.transaction()?;
            tx.execute("DELETE FROM sessions WHERE is_sample = 1", [])?;
            let id = insert_sample(&tx)?;
            tx.commit()?;
            Ok(id)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::{migrations, Store};
    use super::super::{InputMode, SessionStatus};
    use super::SAMPLE_DURATION_MS;
    use rusqlite::Connection;
    use std::sync::Mutex;

    fn store() -> Store {
        let mut conn = Connection::open_in_memory().unwrap();
        migrations::migrate(&mut conn).unwrap();
        Store {
            conn: Mutex::new(conn),
        }
    }

    #[test]
    fn seeds_once_and_records_the_seed_flag() {
        let store = store();
        store.seed_samples_if_needed().unwrap();
        let first = store.list_sessions().unwrap();
        assert_eq!(first.len(), 1);
        assert!(first[0].is_sample);
        assert_eq!(first[0].status, super::super::SessionStatus::Done);
        assert_eq!(
            store.get_setting("samples_seeded").unwrap().as_deref(),
            Some("1")
        );

        store.seed_samples_if_needed().unwrap();
        assert_eq!(store.list_sessions().unwrap().len(), 1);
    }

    #[test]
    fn deleting_a_seeded_sample_does_not_reseed_it() {
        let store = store();
        store.seed_samples_if_needed().unwrap();
        let sample = store.list_sessions().unwrap().remove(0);
        store.delete_session(&sample.id).unwrap();
        store.seed_samples_if_needed().unwrap();
        assert!(store.list_sessions().unwrap().is_empty());
    }

    #[test]
    fn restore_replaces_the_sample_and_stays_idempotent() {
        let store = store();
        let first = store.restore_samples().unwrap();
        let second = store.restore_samples().unwrap();
        let samples: Vec<_> = store
            .list_sessions()
            .unwrap()
            .into_iter()
            .filter(|s| s.is_sample)
            .collect();
        assert_ne!(first, second);
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].id, second);
    }

    #[test]
    fn sample_content_and_defaults_are_not_treated_as_real_session_data() {
        let store = store();
        store.seed_samples_if_needed().unwrap();
        let row = store.list_sessions().unwrap().remove(0);
        let session = store.get_session(&row.id).unwrap().unwrap();
        assert_eq!(session.title, "Sample session");
        assert_eq!(session.input_mode, InputMode::Mic);
        assert_eq!(session.status, SessionStatus::Done);
        assert_eq!(session.duration_ms, Some(SAMPLE_DURATION_MS));
        assert!(session.transcript.starts_with("Therapist:"));
        assert!(!session.transcript.contains("[EXAMPLE]"));
        assert!(session.is_sample);
        assert!(!session.has_audio);
    }
}
