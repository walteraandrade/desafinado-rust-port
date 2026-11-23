use desafinado_core::log_err::LogErr;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};

use super::thin::{ThinCandidate, ids_to_thin};
use super::types::{MarkSavedInput, RecordInput, Unsaved, VersionCause, VersionId, VersionSummary};
use super::word_delta::{count_words, word_delta};

pub const SCHEMA_VERSION: i64 = 1;
const BUSY_TIMEOUT_MS: u32 = 5000;

const SCHEMA: &str = "
  create table files (
    id integer primary key,
    path text unique not null
  );
  create table blobs (
    hash text primary key,
    body blob not null
  );
  create table versions (
    id integer primary key,
    file_id integer not null references files(id),
    at integer not null,
    cause text not null,
    blob_hash text not null references blobs(hash),
    words integer not null,
    words_added integer not null,
    words_removed integer not null,
    saved text
  );
  create index versions_by_file on versions(file_id, at);
  create table drafts (
    file_id integer primary key references files(id),
    text text not null,
    base_hash text not null references blobs(hash),
    at integer not null
  );
";

/// Added after version 1 without a version bump, so that an older build still opens the file.
const PLACES: &str = "
  create table if not exists places (
    file_id integer primary key references files(id),
    place text not null,
    at integer not null
  );
";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct HistoryError(pub String);

impl From<rusqlite::Error> for HistoryError {
    fn from(error: rusqlite::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<std::io::Error> for HistoryError {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}

type Result<T> = std::result::Result<T, HistoryError>;

pub trait HistoryStore {
    fn list(&self, path: &str) -> Result<Vec<VersionSummary>>;
    fn text(&self, path: &str, id: VersionId) -> Result<Option<String>>;
    fn record(&self, path: &str, input: &RecordInput) -> Result<()>;
    fn mark_saved(&self, path: &str, input: &MarkSavedInput) -> Result<()>;
}

pub struct HistoryDb {
    connection: Mutex<Option<Connection>>,
}

fn sha256(text: &str) -> String {
    desafinado_ai::text::lower_hex(&Sha256::digest(text.as_bytes()))
}

fn gzip(text: &str) -> Result<Vec<u8>> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(text.as_bytes())?;
    Ok(encoder.finish()?)
}

fn gunzip(body: &[u8]) -> Result<String> {
    let mut text = String::new();
    GzDecoder::new(body).read_to_string(&mut text)?;
    Ok(text)
}

fn in_transaction<T>(connection: &Connection, run: impl FnOnce() -> Result<T>) -> Result<T> {
    connection.execute_batch("begin immediate")?;
    match run() {
        Ok(value) => {
            connection.execute_batch("commit")?;
            Ok(value)
        }
        Err(error) => {
            connection.execute_batch("rollback").log_warn();
            Err(error)
        }
    }
}

/// Switching to WAL needs a moment alone with the file, and SQLite reports `busy`
/// at once instead of waiting when another connection opens it at the same time.
fn use_write_ahead_log(connection: &Connection) -> Result<()> {
    let deadline = Instant::now() + Duration::from_millis(u64::from(BUSY_TIMEOUT_MS));
    loop {
        match connection.query_row("pragma journal_mode = wal", [], |_| Ok(())) {
            Err(rusqlite::Error::SqliteFailure(failure, _))
                if failure.code == rusqlite::ErrorCode::DatabaseBusy
                    && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            other => return other.map_err(HistoryError::from),
        }
    }
}

fn prepare_schema(connection: &Connection) -> Result<()> {
    connection.execute_batch(&format!("pragma busy_timeout = {BUSY_TIMEOUT_MS}"))?;
    use_write_ahead_log(connection)?;
    connection.execute_batch("pragma synchronous = normal")?;
    in_transaction(connection, || {
        let version: i64 = connection.query_row("pragma user_version", [], |row| row.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(HistoryError(format!(
                "Unsupported History schema version {version}"
            )));
        }
        if version == 0 {
            connection.execute_batch(SCHEMA)?;
            connection.execute_batch(&format!("pragma user_version = {SCHEMA_VERSION}"))?;
        }
        connection.execute_batch(PLACES)?;
        Ok(())
    })
}

fn cause_of(json: &str) -> Result<VersionCause> {
    serde_json::from_str(json)
        .map_err(|error| HistoryError(format!("Unexpected History cause: {error}")))
}

struct Latest {
    hash: String,
    body: Vec<u8>,
}

impl Latest {
    fn text(&self) -> Result<String> {
        gunzip(&self.body)
    }
}

impl HistoryDb {
    pub fn open(file: &Path) -> Result<Self> {
        let connection = Connection::open(file)?;
        if let Err(error) = prepare_schema(&connection) {
            if let Err((_, close_error)) = connection.close() {
                log::warn!("History database did not close cleanly: {close_error}");
            }
            return Err(error);
        }
        Ok(Self {
            connection: Mutex::new(Some(connection)),
        })
    }

    pub fn close(&self) {
        *self
            .connection
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;
    }

    fn with<T>(&self, run: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let guard = self
            .connection
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        match guard.as_ref() {
            Some(connection) => run(connection),
            None => Err(HistoryError("History database is closed".into())),
        }
    }

    fn file_id_of(connection: &Connection, path: &str) -> Result<i64> {
        connection.execute(
            "insert into files (path) values (?) on conflict (path) do nothing",
            params![path],
        )?;
        Ok(connection.query_row(
            "select id from files where path = ?",
            params![path],
            |row| row.get(0),
        )?)
    }

    fn latest_of(connection: &Connection, file_id: i64) -> Result<Option<Latest>> {
        Ok(connection
            .query_row(
                "select v.blob_hash, b.body from versions v join blobs b on b.hash = v.blob_hash where v.file_id = ? order by v.id desc limit 1",
                params![file_id],
                |row| {
                    Ok(Latest {
                        hash: row.get(0)?,
                        body: row.get(1)?,
                    })
                },
            )
            .optional()?)
    }

    fn append(
        connection: &Connection,
        file_id: i64,
        input: &RecordInput,
        hash: &str,
        body: &[u8],
        latest: Option<&Latest>,
    ) -> Result<i64> {
        let before = match latest {
            Some(latest) => latest.text()?,
            None => String::new(),
        };
        let delta = word_delta(&before, &input.text);
        connection.execute(
            "insert or ignore into blobs (hash, body) values (?, ?)",
            params![hash, body],
        )?;
        let cause = if latest.is_none() {
            &VersionCause::Started
        } else {
            &input.cause
        };
        let cause_json = serde_json::to_string(cause).map_err(|e| HistoryError(e.to_string()))?;
        connection.execute(
            "insert into versions (file_id, at, cause, blob_hash, words, words_added, words_removed) values (?, ?, ?, ?, ?, ?, ?)",
            params![
                file_id,
                input.at,
                cause_json,
                hash,
                count_words(&input.text),
                delta.added,
                delta.removed
            ],
        )?;
        Ok(connection.last_insert_rowid())
    }

    pub fn load_unsaved(&self, path: &str) -> Result<Option<Unsaved>> {
        self.with(|connection| {
            let row: Option<(String, Vec<u8>)> = connection
                .query_row(
                    "select d.text, b.body from drafts d join files f on f.id = d.file_id join blobs b on b.hash = d.base_hash where f.path = ?",
                    params![path],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            row.map(|(text, body)| {
                Ok(Unsaved {
                    text,
                    base: gunzip(&body)?,
                })
            })
            .transpose()
        })
    }

    pub fn save_unsaved(&self, path: &str, unsaved: &Unsaved, at: i64) -> Result<()> {
        let base_hash = sha256(&unsaved.base);
        let body = gzip(&unsaved.base)?;
        self.with(|connection| {
            in_transaction(connection, || {
                connection.execute(
                    "insert or ignore into blobs (hash, body) values (?, ?)",
                    params![base_hash, body],
                )?;
                let file_id = Self::file_id_of(connection, path)?;
                connection.execute(
                    "insert into drafts (file_id, text, base_hash, at) values (?, ?, ?, ?) on conflict (file_id) do update set text = excluded.text, base_hash = excluded.base_hash, at = excluded.at",
                    params![file_id, unsaved.text, base_hash, at],
                )?;
                Ok(())
            })
        })
    }

    pub fn clear_unsaved(&self, path: &str) -> Result<()> {
        self.with(|connection| {
            connection.execute(
                "delete from drafts where file_id in (select id from files where path = ?)",
                params![path],
            )?;
            Ok(())
        })
    }

    /// Where the writer was in a file, as the editor saved it.
    pub fn load_place(&self, path: &str) -> Result<Option<String>> {
        self.with(|connection| {
            Ok(connection
                .query_row(
                    "select p.place from places p join files f on f.id = p.file_id where f.path = ?",
                    params![path],
                    |row| row.get(0),
                )
                .optional()?)
        })
    }

    pub fn save_place(&self, path: &str, place: &str, at: i64) -> Result<()> {
        self.with(|connection| {
            in_transaction(connection, || {
                let file_id = Self::file_id_of(connection, path)?;
                connection.execute(
                    "insert into places (file_id, place, at) values (?, ?, ?) on conflict (file_id) do update set place = excluded.place, at = excluded.at",
                    params![file_id, place, at],
                )?;
                Ok(())
            })
        })
    }

    pub fn thin(&self, path: &str, now: i64) -> Result<usize> {
        self.with(|connection| {
            let mut statement = connection.prepare(
                "select v.id, v.at, v.cause, v.saved from versions v join files f on f.id = v.file_id where f.path = ? order by v.id",
            )?;
            let rows = statement.query_map(params![path], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })?;
            let mut candidates = Vec::new();
            for row in rows {
                let (id, at, cause, saved) = row?;
                candidates.push(ThinCandidate {
                    id,
                    at,
                    kind: cause_of(&cause)?.kind().to_owned(),
                    saved: saved.is_some(),
                });
            }
            drop(statement);
            let doomed = ids_to_thin(&candidates, now);
            if doomed.is_empty() {
                return Ok(0);
            }
            in_transaction(connection, || {
                for id in &doomed {
                    connection.execute("delete from versions where id = ?", params![id])?;
                }
                connection.execute(
                    "delete from blobs where hash not in (select blob_hash from versions) and hash not in (select base_hash from drafts)",
                    [],
                )?;
                Ok(())
            })?;
            Ok(doomed.len())
        })
    }
}

impl HistoryStore for HistoryDb {
    fn list(&self, path: &str) -> Result<Vec<VersionSummary>> {
        self.with(|connection| {
            let mut statement = connection.prepare(
                "select v.id, v.at, v.cause, v.words, v.words_added, v.words_removed, v.saved from versions v join files f on f.id = v.file_id where f.path = ? order by v.id",
            )?;
            let rows = statement.query_map(params![path], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, Option<String>>(6)?,
                ))
            })?;
            rows.map(|row| {
                let (id, at, cause, words, words_added, words_removed, saved) = row?;
                Ok(VersionSummary {
                    id,
                    at,
                    cause: cause_of(&cause)?,
                    words,
                    words_added,
                    words_removed,
                    saved,
                })
            })
            .collect()
        })
    }

    fn text(&self, path: &str, id: VersionId) -> Result<Option<String>> {
        self.with(|connection| {
            let body: Option<Vec<u8>> = connection
                .query_row(
                    "select b.body from versions v join files f on f.id = v.file_id join blobs b on b.hash = v.blob_hash where f.path = ? and v.id = ?",
                    params![path, id],
                    |row| row.get(0),
                )
                .optional()?;
            body.map(|body| gunzip(&body)).transpose()
        })
    }

    fn record(&self, path: &str, input: &RecordInput) -> Result<()> {
        let hash = sha256(&input.text);
        let body = gzip(&input.text)?;
        self.with(|connection| {
            in_transaction(connection, || {
                let file_id = Self::file_id_of(connection, path)?;
                let latest = Self::latest_of(connection, file_id)?;
                if latest.as_ref().map(|l| l.hash.as_str()) != Some(hash.as_str()) {
                    Self::append(connection, file_id, input, &hash, &body, latest.as_ref())?;
                }
                Ok(())
            })
        })
    }

    fn mark_saved(&self, path: &str, input: &MarkSavedInput) -> Result<()> {
        let hash = sha256(&input.text);
        let body = gzip(&input.text)?;
        self.with(|connection| {
            in_transaction(connection, || {
                let file_id = Self::file_id_of(connection, path)?;
                let held: Option<i64> = connection
                    .query_row(
                        "select id from versions where file_id = ? and blob_hash = ? order by id desc limit 1",
                        params![file_id, hash],
                        |row| row.get(0),
                    )
                    .optional()?;
                let target = if let Some(id) = held { id } else {
                    let latest = Self::latest_of(connection, file_id)?;
                    let record = RecordInput {
                        at: input.at,
                        cause: VersionCause::Edited,
                        text: input.text.clone(),
                    };
                    Self::append(connection, file_id, &record, &hash, &body, latest.as_ref())?
                };
                connection.execute(
                    "update versions set saved = ? where id = ?",
                    params![input.name, target],
                )?;
                Ok(())
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::AcceptedChange;

    const PATH: &str = "/skills/review/SKILL.md";

    struct Fixture {
        _dir: tempfile::TempDir,
        file: std::path::PathBuf,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("history.sqlite");
        Fixture { _dir: dir, file }
    }

    fn open(fixture: &Fixture) -> HistoryDb {
        HistoryDb::open(&fixture.file).unwrap()
    }

    fn inspect<T>(fixture: &Fixture, run: impl FnOnce(&Connection) -> T) -> T {
        run(&Connection::open(&fixture.file).unwrap())
    }

    fn input(text: &str, at: i64, cause: VersionCause) -> RecordInput {
        RecordInput {
            at,
            cause,
            text: text.into(),
        }
    }

    fn edit(text: &str, at: i64) -> RecordInput {
        input(text, at, VersionCause::Edited)
    }

    fn count(fixture: &Fixture, table: &str) -> i64 {
        inspect(fixture, |c| {
            c.query_row(&format!("select count(*) from {table}"), [], |r| r.get(0))
                .unwrap()
        })
    }

    fn texts(db: &HistoryDb, path: &str) -> Vec<String> {
        db.list(path)
            .unwrap()
            .iter()
            .map(|v| db.text(path, v.id).unwrap().unwrap())
            .collect()
    }

    fn counts(db: &HistoryDb, path: &str) -> Vec<(i64, i64)> {
        db.list(path)
            .unwrap()
            .iter()
            .map(|v| (v.words_added, v.words_removed))
            .collect()
    }

    #[test]
    fn creates_the_tables_of_adr_0015_and_places() {
        let f = fixture();
        open(&f);
        let names: Vec<String> = inspect(&f, |c| {
            let mut s = c
                .prepare("select name from sqlite_master where type in ('table', 'index') and name not like 'sqlite_%' order by name")
                .unwrap();
            s.query_map([], |r| r.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        });
        assert_eq!(
            names,
            [
                "blobs",
                "drafts",
                "files",
                "places",
                "versions",
                "versions_by_file"
            ]
        );
    }

    #[test]
    fn runs_in_wal_mode() {
        let f = fixture();
        open(&f);
        let mode: String = inspect(&f, |c| {
            c.query_row("pragma journal_mode", [], |r| r.get(0))
                .unwrap()
        });
        assert_eq!(mode, "wal");
    }

    #[test]
    fn opens_a_database_it_already_created() {
        let f = fixture();
        let first = open(&f);
        first.record(PATH, &edit("kept", 1)).unwrap();
        first.close();
        assert_eq!(open(&f).list(PATH).unwrap().len(), 1);
    }

    #[test]
    fn creates_the_schema_once_when_connections_race() {
        let f = fixture();
        let file = f.file.clone();
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let file = file.clone();
                std::thread::spawn(move || HistoryDb::open(&file).map(|_| ()))
            })
            .collect();
        for handle in handles {
            handle.join().unwrap().unwrap();
        }
        assert_eq!(
            inspect(&f, |c| c
                .query_row(
                    "select count(*) from sqlite_master where name = 'files'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap()),
            1
        );
    }

    #[test]
    fn keeps_the_latest_place_of_each_file_across_a_restart() {
        let f = fixture();
        let db = open(&f);
        assert_eq!(db.load_place(PATH).unwrap(), None);
        db.save_place(PATH, "{\"first\":1}", 1).unwrap();
        db.save_place(PATH, "{\"second\":2}", 2).unwrap();
        db.save_place("/other.md", "{\"other\":3}", 2).unwrap();
        db.close();
        let db = open(&f);
        assert_eq!(
            db.load_place(PATH).unwrap().as_deref(),
            Some("{\"second\":2}")
        );
        assert_eq!(
            db.load_place("/other.md").unwrap().as_deref(),
            Some("{\"other\":3}")
        );
    }

    #[test]
    fn adds_places_to_a_database_made_before_them_and_keeps_its_version() {
        let f = fixture();
        {
            let connection = Connection::open(&f.file).unwrap();
            connection.execute_batch(SCHEMA).unwrap();
            connection
                .execute_batch(&format!("pragma user_version = {SCHEMA_VERSION}"))
                .unwrap();
        }
        let db = open(&f);
        db.save_place(PATH, "{}", 1).unwrap();
        assert_eq!(db.load_place(PATH).unwrap().as_deref(), Some("{}"));
        db.close();
        let version: i64 = Connection::open(&f.file)
            .unwrap()
            .query_row("pragma user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
    }

    #[test]
    fn refuses_a_database_from_a_newer_schema() {
        let f = fixture();
        Connection::open(&f.file)
            .unwrap()
            .execute_batch("pragma user_version = 99")
            .unwrap();
        let error = HistoryDb::open(&f.file).err().unwrap();
        assert!(error.0.to_lowercase().contains("schema"));
    }

    #[test]
    fn keeps_versions_across_a_restart() {
        let f = fixture();
        let first = open(&f);
        for index in 0..220 {
            first
                .record(PATH, &edit(&format!("version {index}"), index))
                .unwrap();
        }
        first.close();
        let db = open(&f);
        let versions = db.list(PATH).unwrap();
        assert_eq!(versions.len(), 220);
        assert_eq!(
            db.text(PATH, versions[219].id).unwrap().as_deref(),
            Some("version 219")
        );
    }

    #[test]
    fn shares_one_blob_between_equal_texts_within_and_across_files() {
        let f = fixture();
        let db = open(&f);
        db.record(PATH, &edit("alpha", 1)).unwrap();
        db.record(PATH, &edit("beta", 2)).unwrap();
        db.record(PATH, &edit("alpha", 3)).unwrap();
        db.record("/other/SKILL.md", &edit("alpha", 1)).unwrap();
        assert_eq!(count(&f, "blobs"), 2);
        assert_eq!(count(&f, "versions"), 4);
    }

    #[test]
    fn stores_the_text_gzipped_keyed_by_its_sha256() {
        let f = fixture();
        open(&f).record(PATH, &edit("hello ✓", 1)).unwrap();
        let (hash, body): (String, Vec<u8>) = inspect(&f, |c| {
            c.query_row("select hash, body from blobs", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap()
        });
        assert_eq!(gunzip(&body).unwrap(), "hello ✓");
        assert_eq!(hash, sha256("hello ✓"));
        assert_eq!(
            sha256(""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn counts_the_words_when_a_version_is_inserted() {
        let f = fixture();
        open(&f)
            .record(PATH, &edit("one two  three\nfour", 1))
            .unwrap();
        let words: i64 = inspect(&f, |c| {
            c.query_row("select words from versions", [], |r| r.get(0))
                .unwrap()
        });
        assert_eq!(words, 4);
    }

    #[test]
    fn stores_the_cause_as_the_json_of_the_typescript_app() {
        let f = fixture();
        let db = open(&f);
        db.record(PATH, &edit("a", 1)).unwrap();
        db.record(PATH, &input("b", 2, VersionCause::External))
            .unwrap();
        db.record(PATH, &input("c", 3, VersionCause::Restored { from: 1 }))
            .unwrap();
        db.record(
            PATH,
            &input(
                "d",
                4,
                VersionCause::Opened {
                    name: "n.md".into(),
                },
            ),
        )
        .unwrap();
        db.record(
            PATH,
            &input(
                "e",
                5,
                VersionCause::Accepted {
                    changes: vec![AcceptedChange {
                        origin: "ask".into(),
                        instruction: "i".into(),
                        rationale: "r".into(),
                    }],
                },
            ),
        )
        .unwrap();
        let causes: Vec<String> = inspect(&f, |c| {
            let mut s = c.prepare("select cause from versions order by id").unwrap();
            s.query_map([], |r| r.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        });
        assert_eq!(
            causes,
            [
                r#"{"kind":"started"}"#,
                r#"{"kind":"external"}"#,
                r#"{"kind":"restored","from":1}"#,
                r#"{"kind":"opened","name":"n.md"}"#,
                r#"{"kind":"accepted","changes":[{"origin":"ask","instruction":"i","rationale":"r"}]}"#,
            ]
        );
        let listed: Vec<&'static str> = db
            .list(PATH)
            .unwrap()
            .iter()
            .map(|v| v.cause.kind())
            .collect();
        assert_eq!(
            listed,
            ["started", "external", "restored", "opened", "accepted"]
        );
    }

    #[test]
    fn does_not_create_a_file_row_for_a_read() {
        let f = fixture();
        let db = open(&f);
        assert!(db.list("/never/written.md").unwrap().is_empty());
        assert_eq!(db.text("/never/written.md", 1).unwrap(), None);
        assert_eq!(db.load_unsaved("/never/written.md").unwrap(), None);
        assert_eq!(count(&f, "files"), 0);
    }

    #[test]
    fn keeps_a_version_committed_by_a_process_that_never_closed() {
        let f = fixture();
        let db = open(&f);
        db.record(PATH, &edit("committed", 1)).unwrap();
        let other = HistoryDb::open(&f.file).unwrap();
        assert_eq!(other.list(PATH).unwrap().len(), 1);
    }

    #[test]
    fn rejects_use_after_close() {
        let f = fixture();
        let db = open(&f);
        db.close();
        db.close();
        assert!(db.list(PATH).is_err());
    }

    #[test]
    fn lists_nothing_for_a_history_never_written_and_makes_the_first_version_started() {
        let f = fixture();
        let db = open(&f);
        assert!(db.list(PATH).unwrap().is_empty());
        db.record(PATH, &edit("one two three", 5)).unwrap();
        let versions = db.list(PATH).unwrap();
        assert_eq!(versions[0].cause, VersionCause::Started);
        assert_eq!((versions[0].at, versions[0].words), (5, 3));
    }

    #[test]
    fn appends_in_order_and_does_not_record_a_text_equal_to_the_last_version() {
        let f = fixture();
        let db = open(&f);
        for (i, t) in ["a", "b", "b", "a"].iter().enumerate() {
            db.record(PATH, &edit(t, i as i64)).unwrap();
        }
        assert_eq!(texts(&db, PATH), ["a", "b", "a"]);
        let ids: Vec<i64> = db.list(PATH).unwrap().iter().map(|v| v.id).collect();
        assert!(ids.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn counts_words_per_version_against_the_one_before() {
        let f = fixture();
        let db = open(&f);
        for (i, t) in [
            "one two three",
            "one 2 three four",
            "one 2 three four five six",
            "one 2",
        ]
        .iter()
        .enumerate()
        {
            db.record(PATH, &edit(t, i as i64)).unwrap();
        }
        assert_eq!(counts(&db, PATH), [(3, 0), (2, 1), (2, 0), (0, 4)]);
    }

    #[test]
    fn counts_a_change_of_spacing_as_no_words() {
        let f = fixture();
        let db = open(&f);
        db.record(PATH, &edit("one two", 1)).unwrap();
        db.record(PATH, &edit("one  two\n\n", 2)).unwrap();
        assert_eq!(counts(&db, PATH), [(2, 0), (0, 0)]);
    }

    #[test]
    fn counts_a_restored_old_text_against_the_last_version() {
        let f = fixture();
        let db = open(&f);
        db.record(PATH, &edit("one two three", 1)).unwrap();
        db.record(PATH, &edit("one two three four", 2)).unwrap();
        db.record(
            PATH,
            &input("one two three", 3, VersionCause::Restored { from: 1 }),
        )
        .unwrap();
        assert_eq!(counts(&db, PATH).last(), Some(&(0, 1)));
    }

    #[test]
    fn mark_saved_marks_the_newest_version_holding_the_text() {
        let f = fixture();
        let db = open(&f);
        for (i, t) in ["a", "b", "a", "c"].iter().enumerate() {
            db.record(PATH, &edit(t, i as i64)).unwrap();
        }
        db.mark_saved(
            PATH,
            &MarkSavedInput {
                text: "a".into(),
                name: "notes.md".into(),
                at: 200,
            },
        )
        .unwrap();
        let saved: Vec<Option<String>> = db
            .list(PATH)
            .unwrap()
            .into_iter()
            .map(|v| v.saved)
            .collect();
        assert_eq!(saved, [None, None, Some("notes.md".into()), None]);
    }

    #[test]
    fn mark_saved_keeps_earlier_marks_and_records_an_edited_version_when_none_holds_the_text() {
        let f = fixture();
        let db = open(&f);
        db.record(PATH, &edit("a", 1)).unwrap();
        db.record(PATH, &edit("b", 2)).unwrap();
        db.mark_saved(
            PATH,
            &MarkSavedInput {
                text: "a".into(),
                name: "one.md".into(),
                at: 3,
            },
        )
        .unwrap();
        db.mark_saved(
            PATH,
            &MarkSavedInput {
                text: "b".into(),
                name: "two.md".into(),
                at: 4,
            },
        )
        .unwrap();
        db.mark_saved(
            PATH,
            &MarkSavedInput {
                text: "new text".into(),
                name: "three.md".into(),
                at: 300,
            },
        )
        .unwrap();
        let versions = db.list(PATH).unwrap();
        assert_eq!(versions.len(), 3);
        assert_eq!(versions[0].saved.as_deref(), Some("one.md"));
        assert_eq!(versions[1].saved.as_deref(), Some("two.md"));
        assert_eq!(versions[2].cause, VersionCause::Edited);
        assert_eq!((versions[2].at, versions[2].words), (300, 2));
        assert_eq!(versions[2].saved.as_deref(), Some("three.md"));
        assert_eq!((versions[2].words_added, versions[2].words_removed), (2, 1));
    }

    #[test]
    fn mark_saved_starts_the_history_when_the_saved_text_is_its_first_version() {
        let f = fixture();
        let db = open(&f);
        db.mark_saved(
            PATH,
            &MarkSavedInput {
                text: "only these".into(),
                name: "a.md".into(),
                at: 1,
            },
        )
        .unwrap();
        let versions = db.list(PATH).unwrap();
        assert_eq!(versions[0].cause, VersionCause::Started);
        assert_eq!(versions[0].saved.as_deref(), Some("a.md"));
        assert_eq!((versions[0].words_added, versions[0].words_removed), (2, 0));
        assert_eq!(
            db.text(PATH, versions[0].id).unwrap().as_deref(),
            Some("only these")
        );
    }

    #[test]
    fn mark_saved_does_not_count_a_version_it_only_marked() {
        let f = fixture();
        let db = open(&f);
        db.record(PATH, &edit("one two", 1)).unwrap();
        db.record(PATH, &edit("one", 2)).unwrap();
        db.mark_saved(
            PATH,
            &MarkSavedInput {
                text: "one two".into(),
                name: "a.md".into(),
                at: 9,
            },
        )
        .unwrap();
        assert_eq!(counts(&db, PATH), [(2, 0), (0, 1)]);
    }

    #[test]
    fn keeps_every_version_and_loads_texts_on_demand() {
        let f = fixture();
        let db = open(&f);
        for i in 0..250 {
            db.record(PATH, &edit(&format!("version {i}"), i)).unwrap();
        }
        let versions = db.list(PATH).unwrap();
        assert_eq!(versions.len(), 250);
        assert_eq!(
            db.text(PATH, versions[249].id).unwrap().as_deref(),
            Some("version 249")
        );
        assert_eq!(db.text(PATH, 987_654).unwrap(), None);
    }

    #[test]
    fn keeps_the_history_of_each_file_apart() {
        let f = fixture();
        let db = open(&f);
        db.record(PATH, &edit("same", 1)).unwrap();
        db.record("other", &edit("same", 1)).unwrap();
        assert_eq!(db.list("other").unwrap().len(), 1);
        assert_eq!(db.list("other").unwrap()[0].cause, VersionCause::Started);
        let mine = db.list(PATH).unwrap()[0].id;
        assert_eq!(db.text("other", mine).unwrap(), None);
        db.mark_saved(
            "other",
            &MarkSavedInput {
                text: "same".into(),
                name: "b.md".into(),
                at: 9,
            },
        )
        .unwrap();
        assert_eq!(db.list(PATH).unwrap()[0].saved, None);
        assert_eq!(db.list("other").unwrap()[0].saved.as_deref(), Some("b.md"));
    }

    fn unsaved(text: &str, base: &str) -> Unsaved {
        Unsaved {
            text: text.into(),
            base: base.into(),
        }
    }

    #[test]
    fn saves_and_loads_the_unsaved_text_with_the_text_it_started_from() {
        let f = fixture();
        let db = open(&f);
        assert_eq!(db.load_unsaved(PATH).unwrap(), None);
        db.save_unsaved(PATH, &unsaved("typed so far", "the disk text"), 10)
            .unwrap();
        assert_eq!(
            db.load_unsaved(PATH).unwrap(),
            Some(unsaved("typed so far", "the disk text"))
        );
    }

    #[test]
    fn keeps_only_the_latest_unsaved_text_of_a_file() {
        let f = fixture();
        let db = open(&f);
        db.save_unsaved(PATH, &unsaved("one", "base one"), 1)
            .unwrap();
        db.save_unsaved(PATH, &unsaved("two", "base two"), 2)
            .unwrap();
        assert_eq!(
            db.load_unsaved(PATH).unwrap(),
            Some(unsaved("two", "base two"))
        );
        assert_eq!(count(&f, "drafts"), 1);
    }

    #[test]
    fn keeps_unsaved_texts_apart_and_clears_one_file_only() {
        let f = fixture();
        let db = open(&f);
        db.save_unsaved(PATH, &unsaved("mine", "on disk"), 1)
            .unwrap();
        db.save_unsaved("/other.md", &unsaved("theirs", "on disk"), 1)
            .unwrap();
        db.clear_unsaved(PATH).unwrap();
        assert_eq!(db.load_unsaved(PATH).unwrap(), None);
        assert_eq!(
            db.load_unsaved("/other.md").unwrap().unwrap().text,
            "theirs"
        );
    }

    #[test]
    fn shares_the_draft_base_blob_with_the_version_holding_the_same_text() {
        let f = fixture();
        let db = open(&f);
        db.record(PATH, &edit("the disk text", 1)).unwrap();
        db.save_unsaved(PATH, &unsaved("typed", "the disk text"), 1)
            .unwrap();
        assert_eq!(count(&f, "blobs"), 1);
        let hash: String = inspect(&f, |c| {
            c.query_row("select base_hash from drafts", [], |r| r.get(0))
                .unwrap()
        });
        assert_eq!(hash, sha256("the disk text"));
    }

    #[test]
    fn keeps_an_empty_base_and_survives_a_reopen() {
        let f = fixture();
        let first = open(&f);
        first
            .save_unsaved("scratch:document", &unsaved("typed", ""), 1)
            .unwrap();
        first
            .save_unsaved(PATH, &unsaved("before the crash", "base"), 1)
            .unwrap();
        first.close();
        let db = open(&f);
        assert_eq!(
            db.load_unsaved("scratch:document").unwrap(),
            Some(unsaved("typed", ""))
        );
        assert_eq!(
            db.load_unsaved(PATH).unwrap(),
            Some(unsaved("before the crash", "base"))
        );
    }

    #[test]
    fn unsaved_text_is_readable_by_another_connection_while_the_first_is_open() {
        let f = fixture();
        let db = open(&f);
        db.save_unsaved(PATH, &unsaved("killed mid-edit", "b"), 1)
            .unwrap();
        let text: String = inspect(&f, |c| {
            c.query_row("select text from drafts", [], |r| r.get(0))
                .unwrap()
        });
        assert_eq!(text, "killed mid-edit");
        drop(db);
    }

    const DAY: i64 = 86_400_000;
    const NOW: i64 = 200 * DAY;

    #[test]
    fn thin_removes_old_edited_versions_inside_the_same_hour_and_their_orphan_blobs() {
        let f = fixture();
        let db = open(&f);
        let base = NOW - 10 * DAY;
        db.record(PATH, &edit("one", base)).unwrap();
        db.record(PATH, &edit("one two", base + 60_000)).unwrap();
        db.record(PATH, &edit("one two three", base + 120_000))
            .unwrap();
        db.record(PATH, &edit("one two three four", base + 180_000))
            .unwrap();
        db.record(PATH, &edit("one two three four five", NOW))
            .unwrap();
        assert_eq!(db.thin(PATH, NOW).unwrap(), 2);
        let words: Vec<i64> = db.list(PATH).unwrap().iter().map(|v| v.words).collect();
        assert_eq!(words, [1, 4, 5]);
        assert_eq!(count(&f, "blobs"), 3);
    }

    #[test]
    fn thin_keeps_a_text_a_draft_still_builds_on() {
        let f = fixture();
        let db = open(&f);
        let base = NOW - 10 * DAY;
        db.record(PATH, &edit("old", base)).unwrap();
        db.record(PATH, &edit("old text", base + 1000)).unwrap();
        db.record(PATH, &edit("newest", NOW)).unwrap();
        db.save_unsaved(PATH, &unsaved("old text plus", "old"), NOW)
            .unwrap();
        db.thin(PATH, NOW).unwrap();
        assert_eq!(
            db.load_unsaved(PATH).unwrap(),
            Some(unsaved("old text plus", "old"))
        );
    }

    #[test]
    fn thin_does_nothing_for_a_history_with_nothing_to_thin() {
        let f = fixture();
        let db = open(&f);
        db.record(PATH, &edit("one", NOW - 1000)).unwrap();
        assert_eq!(db.thin(PATH, NOW).unwrap(), 0);
    }
}
