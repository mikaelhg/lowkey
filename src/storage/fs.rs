use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};

use bytes::Bytes;
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

use super::{StorageError, Usage, UserRecord};
use crate::domain::{ContentType, DbName, DocName, Document, Username};

type Result<T> = std::result::Result<T, StorageError>;

const USER_FILE: &str = "user.json";

/// Longest content type line we expect at the start of a document file.
const MAX_HEADER_LEN: u64 = 128;

/// Makes temporary names unique within this process.
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A store rooted at a directory on the local filesystem.
#[derive(Debug, Clone)]
pub struct FsStore {
    root: PathBuf,
}

impl FsStore {
    /// Opens the store, creating the root directory if needed and removing
    /// leftovers from writes or deletions interrupted by a crash.
    pub async fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root).await?;
        remove_leftovers(&root, 3).await?;
        Ok(Self { root })
    }

    // Users

    pub async fn create_user(&self, user: &Username, record: &UserRecord) -> Result<()> {
        fs::create_dir(self.user_dir(user)).await?;
        self.write_user(user, record).await
    }

    pub async fn read_user(&self, user: &Username) -> Result<UserRecord> {
        let path = self.user_file(user);
        let contents = fs::read(&path).await?;
        serde_json::from_slice(&contents).map_err(|error| StorageError::Corrupt {
            path,
            reason: error.to_string(),
        })
    }

    /// Replaces the record of an existing user.
    pub async fn write_user(&self, user: &Username, record: &UserRecord) -> Result<()> {
        let contents = serde_json::to_vec_pretty(record).expect("user record serializes");
        write_atomic(&self.user_file(user), &contents).await
    }

    /// Deletes a user and everything they own.
    pub async fn delete_user(&self, user: &Username) -> Result<()> {
        remove_dir_atomic(&self.user_dir(user)).await
    }

    // Databases

    pub async fn list_dbs(&self, user: &Username) -> Result<Vec<DbName>> {
        list_names(&self.user_dir(user), EntryKind::Dir).await
    }

    pub async fn create_db(&self, user: &Username, db: &DbName) -> Result<()> {
        Ok(fs::create_dir(self.db_dir(user, db)).await?)
    }

    /// Deletes a database and all of its documents.
    pub async fn delete_db(&self, user: &Username, db: &DbName) -> Result<()> {
        remove_dir_atomic(&self.db_dir(user, db)).await
    }

    // Documents

    pub async fn list_docs(&self, user: &Username, db: &DbName) -> Result<Vec<DocName>> {
        list_names(&self.db_dir(user, db), EntryKind::File).await
    }

    pub async fn read_doc(&self, user: &Username, db: &DbName, doc: &DocName) -> Result<Document> {
        let path = self.doc_file(user, db, doc);
        let contents = fs::read(&path).await?;
        decode_document(contents).map_err(|reason| StorageError::Corrupt { path, reason })
    }

    /// Creates or replaces a document in an existing database.
    pub async fn write_doc(
        &self,
        user: &Username,
        db: &DbName,
        doc: &DocName,
        document: &Document,
    ) -> Result<()> {
        write_atomic(&self.doc_file(user, db, doc), &encode_document(document)).await
    }

    pub async fn delete_doc(&self, user: &Username, db: &DbName, doc: &DocName) -> Result<()> {
        Ok(fs::remove_file(self.doc_file(user, db, doc)).await?)
    }

    // Usage

    /// Walks the whole store and reports what each user stores. Meant to run
    /// once at startup to seed quota tracking.
    pub async fn disk_usage(&self) -> Result<HashMap<Username, Usage>> {
        let mut usage = HashMap::new();
        for user in list_names::<Username>(&self.root, EntryKind::Dir).await? {
            let mut total = Usage::default();
            for db in self.list_dbs(&user).await? {
                total.databases += 1;
                for doc in self.list_docs(&user, &db).await? {
                    total.documents += 1;
                    total.bytes += body_len(&self.doc_file(&user, &db, &doc)).await?;
                }
            }
            usage.insert(user, total);
        }
        Ok(usage)
    }

    // Paths

    fn user_dir(&self, user: &Username) -> PathBuf {
        self.root.join(user.as_ref())
    }

    fn user_file(&self, user: &Username) -> PathBuf {
        self.user_dir(user).join(USER_FILE)
    }

    fn db_dir(&self, user: &Username, db: &DbName) -> PathBuf {
        self.user_dir(user).join(db.as_ref())
    }

    fn doc_file(&self, user: &Username, db: &DbName, doc: &DocName) -> PathBuf {
        self.db_dir(user, db).join(doc.as_ref())
    }
}

// Document file format: the canonical MIME type, a newline, then the body.

fn encode_document(document: &Document) -> Vec<u8> {
    let header = document.content_type.as_mime();
    let mut contents = Vec::with_capacity(header.len() + 1 + document.body.len());
    contents.extend_from_slice(header.as_bytes());
    contents.push(b'\n');
    contents.extend_from_slice(&document.body);
    contents
}

fn decode_document(contents: Vec<u8>) -> std::result::Result<Document, String> {
    let newline = contents
        .iter()
        .position(|&b| b == b'\n')
        .ok_or("missing content type line")?;
    let header = std::str::from_utf8(&contents[..newline]).map_err(|e| e.to_string())?;
    let content_type =
        ContentType::from_mime(header).ok_or_else(|| format!("unknown content type {header:?}"))?;
    let body = Bytes::from(contents).slice(newline + 1..);
    Ok(Document { content_type, body })
}

/// The size of a document's body, without reading the body itself.
async fn body_len(path: &Path) -> Result<u64> {
    let file = fs::File::open(path).await?;
    let file_len = file.metadata().await?.len();
    let mut header = Vec::new();
    BufReader::new(file.take(MAX_HEADER_LEN))
        .read_until(b'\n', &mut header)
        .await?;
    if header.last() != Some(&b'\n') {
        return Err(StorageError::Corrupt {
            path: path.to_owned(),
            reason: "missing content type line".into(),
        });
    }
    Ok(file_len - header.len() as u64)
}

// Filesystem helpers

#[derive(Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    File,
    Dir,
}

/// Lists the entries of `dir` of the given kind whose names are valid `T`s,
/// sorted. Anything else, such as `user.json` or temporary files, is skipped.
async fn list_names<T: FromStr + Ord>(dir: &Path, kind: EntryKind) -> Result<Vec<T>> {
    let mut names = Vec::new();
    let mut entries = fs::read_dir(dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let file_type = entry.file_type().await?;
        let matches = match kind {
            EntryKind::File => file_type.is_file(),
            EntryKind::Dir => file_type.is_dir(),
        };
        let name = entry.file_name().to_str().and_then(|s| s.parse().ok());
        if let (true, Some(name)) = (matches, name) {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

/// A unique hidden path in `dir`, for temporary files and pending deletions.
fn hidden_path(dir: &Path, purpose: &str) -> PathBuf {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    dir.join(format!(".{purpose}-{}-{n}", std::process::id()))
}

/// Writes `contents` to `path` so that readers see either the old file or
/// the complete new one, even across a crash.
async fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    let dir = path.parent().expect("stored paths have a parent");
    let temp = hidden_path(dir, "tmp");
    let result: io::Result<()> = async {
        let mut file = fs::File::create_new(&temp).await?;
        file.write_all(contents).await?;
        file.sync_all().await?;
        fs::rename(&temp, path).await?;
        sync_dir(dir).await
    }
    .await;
    if result.is_err() {
        let _ = fs::remove_file(&temp).await;
    }
    Ok(result?)
}

/// Removes a directory tree. It is first renamed out of the way, so it
/// disappears from view at once; a crash mid-removal leaves only a hidden
/// leftover that `remove_leftovers` cleans up.
async fn remove_dir_atomic(path: &Path) -> Result<()> {
    let parent = path.parent().expect("stored paths have a parent");
    let doomed = hidden_path(parent, "deleted");
    fs::rename(path, &doomed).await?;
    sync_dir(parent).await?;
    Ok(fs::remove_dir_all(&doomed).await?)
}

/// Makes a rename or file creation in `dir` durable.
async fn sync_dir(dir: &Path) -> io::Result<()> {
    fs::File::open(dir).await?.sync_all().await
}

/// Removes hidden entries up to `depth` levels below `dir`.
async fn remove_leftovers(dir: &Path, depth: usize) -> Result<()> {
    let mut entries = fs::read_dir(dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let is_dir = entry.file_type().await?.is_dir();
        if entry.file_name().to_string_lossy().starts_with('.') {
            tracing::warn!(path = %path.display(), "removing leftover from interrupted operation");
            if is_dir {
                fs::remove_dir_all(&path).await?;
            } else {
                fs::remove_file(&path).await?;
            }
        } else if is_dir && depth > 1 {
            Box::pin(remove_leftovers(&path, depth - 1)).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name<T: FromStr>(s: &str) -> T
    where
        T::Err: std::fmt::Debug,
    {
        s.parse().unwrap()
    }

    fn json(body: &'static str) -> Document {
        Document {
            content_type: ContentType::Json,
            body: Bytes::from_static(body.as_bytes()),
        }
    }

    async fn store_with_db() -> (tempfile::TempDir, FsStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = FsStore::open(dir.path()).await.unwrap();
        let record = UserRecord {
            password_hash: "hash".into(),
        };
        store.create_user(&name("alice"), &record).await.unwrap();
        store
            .create_db(&name("alice"), &name("notes"))
            .await
            .unwrap();
        (dir, store)
    }

    #[test]
    fn document_encoding_round_trips() {
        for content_type in [ContentType::Json, ContentType::Text, ContentType::Binary] {
            let document = Document {
                content_type,
                body: Bytes::from_static(b"line one\nline two\n\0"),
            };
            assert_eq!(
                decode_document(encode_document(&document)).unwrap(),
                document
            );
        }
    }

    #[test]
    fn corrupt_documents_are_rejected() {
        assert!(decode_document(b"no newline".to_vec()).is_err());
        assert!(decode_document(b"image/png\nbody".to_vec()).is_err());
    }

    #[tokio::test]
    async fn creating_twice_is_already_exists() {
        let (_dir, store) = store_with_db().await;
        let record = UserRecord {
            password_hash: "other".into(),
        };
        assert!(matches!(
            store.create_user(&name("alice"), &record).await,
            Err(StorageError::AlreadyExists)
        ));
        assert!(matches!(
            store.create_db(&name("alice"), &name("notes")).await,
            Err(StorageError::AlreadyExists)
        ));
    }

    #[tokio::test]
    async fn missing_things_are_not_found() {
        let (_dir, store) = store_with_db().await;
        let (alice, bob) = (name("alice"), name("bob"));
        let (notes, other) = (name("notes"), name("other"));
        assert!(matches!(
            store.read_user(&bob).await,
            Err(StorageError::NotFound)
        ));
        assert!(matches!(
            store.create_db(&bob, &notes).await,
            Err(StorageError::NotFound)
        ));
        assert!(matches!(
            store.read_doc(&alice, &notes, &name("x")).await,
            Err(StorageError::NotFound)
        ));
        assert!(matches!(
            store
                .write_doc(&alice, &other, &name("x"), &json("1"))
                .await,
            Err(StorageError::NotFound)
        ));
        assert!(matches!(
            store.delete_db(&alice, &other).await,
            Err(StorageError::NotFound)
        ));
    }

    #[tokio::test]
    async fn writing_a_document_replaces_it() {
        let (_dir, store) = store_with_db().await;
        let (alice, notes, doc) = (name("alice"), name("notes"), name("doc"));
        store
            .write_doc(&alice, &notes, &doc, &json("1"))
            .await
            .unwrap();
        store
            .write_doc(&alice, &notes, &doc, &json("2"))
            .await
            .unwrap();
        assert_eq!(
            store.read_doc(&alice, &notes, &doc).await.unwrap(),
            json("2")
        );
    }

    #[tokio::test]
    async fn deleting_removes_documents_databases_and_users() {
        let (_dir, store) = store_with_db().await;
        let (alice, notes, doc) = (name("alice"), name("notes"), name("doc"));
        store
            .write_doc(&alice, &notes, &doc, &json("1"))
            .await
            .unwrap();

        store.delete_doc(&alice, &notes, &doc).await.unwrap();
        assert!(store.list_docs(&alice, &notes).await.unwrap().is_empty());

        store.delete_db(&alice, &notes).await.unwrap();
        assert!(store.list_dbs(&alice).await.unwrap().is_empty());

        store.delete_user(&alice).await.unwrap();
        assert!(matches!(
            store.read_user(&alice).await,
            Err(StorageError::NotFound)
        ));
        assert!(store.disk_usage().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn listings_are_sorted_and_skip_non_names() {
        let (dir, store) = store_with_db().await;
        let alice = name("alice");
        store.create_db(&alice, &name("b")).await.unwrap();
        store.create_db(&alice, &name("a")).await.unwrap();
        std::fs::create_dir(dir.path().join("alice/not.a.db")).unwrap();
        assert_eq!(
            store.list_dbs(&alice).await.unwrap(),
            vec![name("a"), name("b"), name("notes")]
        );
    }

    #[tokio::test]
    async fn opening_removes_interrupted_leftovers() {
        let (dir, store) = store_with_db().await;
        let db_dir = dir.path().join("alice/notes");
        std::fs::write(db_dir.join(".tmp-1-1"), b"partial").unwrap();
        std::fs::create_dir(dir.path().join(".deleted-1-2")).unwrap();
        drop(store);

        FsStore::open(dir.path()).await.unwrap();

        assert!(!db_dir.join(".tmp-1-1").exists());
        assert!(!dir.path().join(".deleted-1-2").exists());
        assert!(db_dir.exists());
    }
}
