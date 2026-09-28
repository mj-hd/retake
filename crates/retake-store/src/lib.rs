use chrono::Utc;
use parking_lot::Mutex;
use rand::Rng;
use retake_core::{Annotation, Generation, ReviewResult, StoredSnapshot};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::watch;

#[derive(thiserror::Error, Debug)]
pub enum StoreError {
    #[error("not found")]
    NotFound,
    #[error("invalid state")]
    InvalidState,
    #[error("other: {0}")]
    Other(String),
}

pub struct ReviewStore {
    state: Arc<Mutex<StoreState>>,
}

#[derive(Default)]
struct StoreState {
    reviews: HashMap<String, ReviewRecord>,
}

struct ReviewRecord {
    _title: Option<String>,
    state: ReviewState,
    result: Option<ReviewResult>,
    token_hash: Vec<u8>,
    revisions: Vec<RevisionRecord>,
    feedback: Vec<ReviewFeedback>,
    updating_at: Option<String>,
    messages: Vec<ReviewMessage>,
    assets: HashMap<String, Vec<u8>>,
    notify: watch::Sender<u64>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReviewState {
    Pending,
    Submitted,
    Cancelled,
}

#[derive(Clone)]
struct RevisionRecord {
    number: i64,
    created_at: String,
    snapshots: Vec<StoredSnapshot>,
}

#[derive(Clone)]
pub struct ReviewRevision {
    pub number: i64,
    pub created_at: String,
    pub snapshots: Vec<StoredSnapshot>,
}

#[derive(Clone)]
pub struct ReviewFeedback {
    pub number: i64,
    pub annotations: Vec<Annotation>,
}

#[derive(Clone, serde::Serialize)]
pub struct ReviewMessage {
    pub id: String,
    pub kind: String,
    pub text: String,
    pub reply: Option<String>,
    pub handed_off_at: Option<String>,
    pub created_at: String,
}

impl Default for ReviewStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ReviewStore {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(StoreState::default())),
        }
    }

    fn hash_token(token: &str) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        hasher.finalize().to_vec()
    }

    pub fn generate_token(&self) -> String {
        let mut rng = rand::thread_rng();
        let bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
        hex::encode(bytes)
    }

    pub fn create_review(
        &self,
        title: Option<String>,
        snapshots: Vec<(StoredSnapshot, Vec<u8>)>,
        token: String,
    ) -> Result<String, StoreError> {
        let id = uuid::Uuid::new_v4().to_string();
        let mut assets = HashMap::new();
        let mut stored = Vec::with_capacity(snapshots.len());
        for (position, (mut snapshot, bytes)) in snapshots.into_iter().enumerate() {
            if assets.contains_key(&snapshot.id) {
                return Err(StoreError::InvalidState);
            }
            snapshot.review_id = id.clone();
            snapshot.position = position as i32;
            snapshot.asset_path = snapshot.id.clone();
            assets.insert(snapshot.id.clone(), bytes);
            stored.push(snapshot);
        }
        let created_at = Utc::now().to_rfc3339();
        let (notify, _) = watch::channel(0);
        let review = ReviewRecord {
            _title: title,
            state: ReviewState::Pending,
            result: None,
            token_hash: Self::hash_token(&token),
            revisions: vec![RevisionRecord {
                number: 1,
                created_at,
                snapshots: stored,
            }],
            feedback: vec![],
            updating_at: None,
            messages: vec![],
            assets,
            notify,
        };
        self.state.lock().reviews.insert(id.clone(), review);
        Ok(id)
    }

    pub fn get_review_state(&self, review_id: &str) -> Result<ReviewResult, StoreError> {
        let state = self.state.lock();
        let review = state.reviews.get(review_id).ok_or(StoreError::NotFound)?;
        Ok(Self::review_result(review_id, review))
    }

    fn review_result(review_id: &str, review: &ReviewRecord) -> ReviewResult {
        match review.state {
            ReviewState::Submitted => review.result.clone().unwrap_or(ReviewResult {
                status: "submitted".into(),
                review_id: review_id.into(),
                annotations: None,
                generation: None,
            }),
            ReviewState::Pending => ReviewResult {
                status: "pending".into(),
                review_id: review_id.into(),
                annotations: None,
                generation: None,
            },
            ReviewState::Cancelled => ReviewResult {
                status: "cancelled".into(),
                review_id: review_id.into(),
                annotations: None,
                generation: None,
            },
        }
    }

    pub fn submit_review(
        &self,
        review_id: &str,
        annotations: Vec<Annotation>,
        generation: Generation,
    ) -> Result<ReviewResult, StoreError> {
        let result = {
            let mut state = self.state.lock();
            let review = state
                .reviews
                .get_mut(review_id)
                .ok_or(StoreError::NotFound)?;
            if review.state != ReviewState::Pending {
                return Ok(Self::review_result(review_id, review));
            }
            let result = ReviewResult {
                status: "submitted".into(),
                review_id: review_id.into(),
                annotations: Some(annotations.clone()),
                generation: Some(generation),
            };
            review.state = ReviewState::Submitted;
            review.result = Some(result.clone());
            let revision = review
                .revisions
                .last()
                .ok_or(StoreError::InvalidState)?
                .number;
            review.feedback.push(ReviewFeedback {
                number: revision,
                annotations,
            });
            review.updating_at = Some(Utc::now().to_rfc3339());
            Self::notify(review);
            result
        };
        Ok(result)
    }

    pub fn cancel_review(&self, review_id: &str) -> Result<ReviewResult, StoreError> {
        let result = {
            let mut state = self.state.lock();
            let review = state
                .reviews
                .get_mut(review_id)
                .ok_or(StoreError::NotFound)?;
            if review.state != ReviewState::Pending {
                return Ok(Self::review_result(review_id, review));
            }
            review.state = ReviewState::Cancelled;
            review.result = None;
            review.updating_at = None;
            Self::notify(review);
            Self::review_result(review_id, review)
        };
        Ok(result)
    }

    pub fn list_snapshots(&self, review_id: &str) -> Result<Vec<StoredSnapshot>, StoreError> {
        let state = self.state.lock();
        let review = state.reviews.get(review_id).ok_or(StoreError::NotFound)?;
        let snapshots = review
            .revisions
            .first()
            .ok_or(StoreError::NotFound)?
            .snapshots
            .clone();
        if snapshots.is_empty() {
            Err(StoreError::NotFound)
        } else {
            Ok(snapshots)
        }
    }

    pub fn list_revisions(&self, review_id: &str) -> Result<Vec<ReviewRevision>, StoreError> {
        let state = self.state.lock();
        let review = state.reviews.get(review_id).ok_or(StoreError::NotFound)?;
        let mut revisions = review
            .revisions
            .iter()
            .map(|revision| ReviewRevision {
                number: revision.number,
                created_at: revision.created_at.clone(),
                snapshots: revision.snapshots.clone(),
            })
            .collect::<Vec<_>>();
        if let Some(created_at) = &review.updating_at {
            revisions.push(ReviewRevision {
                number: revisions
                    .last()
                    .map(|revision| revision.number + 1)
                    .unwrap_or(1),
                created_at: created_at.clone(),
                snapshots: vec![],
            });
        }
        Ok(revisions)
    }

    pub fn list_feedback(&self, review_id: &str) -> Result<Vec<ReviewFeedback>, StoreError> {
        let state = self.state.lock();
        let review = state.reviews.get(review_id).ok_or(StoreError::NotFound)?;
        Ok(review.feedback.clone())
    }

    pub fn begin_update(&self, review_id: &str) -> Result<(), StoreError> {
        let mut state = self.state.lock();
        let review = state
            .reviews
            .get_mut(review_id)
            .ok_or(StoreError::NotFound)?;
        if !matches!(review.state, ReviewState::Submitted | ReviewState::Pending) {
            return Err(StoreError::InvalidState);
        }
        if review.updating_at.is_none() {
            review.updating_at = Some(Utc::now().to_rfc3339());
        }
        Ok(())
    }

    pub fn abort_update(&self, review_id: &str) -> Result<(), StoreError> {
        let mut state = self.state.lock();
        let review = state
            .reviews
            .get_mut(review_id)
            .ok_or(StoreError::NotFound)?;
        review.updating_at = None;
        Ok(())
    }

    pub fn is_updating(&self, review_id: &str) -> Result<bool, StoreError> {
        let state = self.state.lock();
        let review = state.reviews.get(review_id).ok_or(StoreError::NotFound)?;
        Ok(review.updating_at.is_some())
    }

    pub fn add_message(
        &self,
        review_id: &str,
        kind: &str,
        text: &str,
    ) -> Result<String, StoreError> {
        let text = text.trim();
        let invalid_progress = kind == "progress"
            && (text.chars().count() > 60
                || text.contains('\n')
                || text.contains('\r')
                || text.ends_with('.')
                || text.ends_with('。'));
        if !matches!(kind, "progress" | "question")
            || text.is_empty()
            || text.chars().count() > 2000
            || invalid_progress
        {
            return Err(StoreError::InvalidState);
        }
        let mut state = self.state.lock();
        let review = state
            .reviews
            .get_mut(review_id)
            .ok_or(StoreError::NotFound)?;
        let id = uuid::Uuid::new_v4().to_string();
        review.messages.push(ReviewMessage {
            id: id.clone(),
            kind: kind.into(),
            text: text.into(),
            reply: None,
            handed_off_at: None,
            created_at: Utc::now().to_rfc3339(),
        });
        Ok(id)
    }

    pub fn list_messages(&self, review_id: &str) -> Result<Vec<ReviewMessage>, StoreError> {
        let state = self.state.lock();
        let review = state.reviews.get(review_id).ok_or(StoreError::NotFound)?;
        Ok(review.messages.clone())
    }

    pub fn get_reply(
        &self,
        review_id: &str,
        message_id: &str,
    ) -> Result<Option<String>, StoreError> {
        Ok(self.get_message_response(review_id, message_id)?.0)
    }

    pub fn get_message_response(
        &self,
        review_id: &str,
        message_id: &str,
    ) -> Result<(Option<String>, bool), StoreError> {
        let state = self.state.lock();
        let review = state.reviews.get(review_id).ok_or(StoreError::NotFound)?;
        let message = review
            .messages
            .iter()
            .find(|message| message.id == message_id && message.kind == "question")
            .ok_or(StoreError::NotFound)?;
        Ok((message.reply.clone(), message.handed_off_at.is_some()))
    }

    pub fn has_chat_handoff(&self, review_id: &str) -> Result<bool, StoreError> {
        let state = self.state.lock();
        let review = state.reviews.get(review_id).ok_or(StoreError::NotFound)?;
        let latest_revision = review
            .revisions
            .last()
            .map(|revision| revision.created_at.as_str())
            .unwrap_or("");
        Ok(review.messages.iter().any(|message| {
            message
                .handed_off_at
                .as_deref()
                .is_some_and(|at| at >= latest_revision)
        }))
    }

    pub fn handoff_message(&self, review_id: &str, message_id: &str) -> Result<(), StoreError> {
        {
            let mut state = self.state.lock();
            let review = state
                .reviews
                .get_mut(review_id)
                .ok_or(StoreError::NotFound)?;
            let message = review
                .messages
                .iter_mut()
                .find(|message| message.id == message_id && message.kind == "question")
                .ok_or(StoreError::InvalidState)?;
            if message.reply.is_some() || message.handed_off_at.is_some() {
                return Err(StoreError::InvalidState);
            }
            message.handed_off_at = Some(Utc::now().to_rfc3339());
            Self::notify(review);
        }
        Ok(())
    }

    pub fn reply_message(
        &self,
        review_id: &str,
        message_id: &str,
        text: &str,
    ) -> Result<(), StoreError> {
        if text.trim().is_empty() || text.len() > 4000 {
            return Err(StoreError::InvalidState);
        }
        {
            let mut state = self.state.lock();
            let review = state
                .reviews
                .get_mut(review_id)
                .ok_or(StoreError::NotFound)?;
            let message = review
                .messages
                .iter_mut()
                .find(|message| message.id == message_id && message.kind == "question")
                .ok_or(StoreError::InvalidState)?;
            if message.reply.is_some() || message.handed_off_at.is_some() {
                return Err(StoreError::InvalidState);
            }
            message.reply = Some(text.trim().into());
            Self::notify(review);
        }
        Ok(())
    }

    pub fn append_revision(
        &self,
        review_id: &str,
        snapshots: Vec<(StoredSnapshot, Vec<u8>)>,
    ) -> Result<i64, StoreError> {
        let mut state = self.state.lock();
        let review = state
            .reviews
            .get_mut(review_id)
            .ok_or(StoreError::NotFound)?;
        if !matches!(review.state, ReviewState::Submitted | ReviewState::Pending)
            || (review.state == ReviewState::Pending && review.feedback.is_empty())
        {
            return Err(StoreError::InvalidState);
        }
        let previous = review.revisions.last().ok_or(StoreError::InvalidState)?;
        if snapshots.len() != previous.snapshots.len()
            || snapshots
                .iter()
                .zip(&previous.snapshots)
                .any(|((next, _), current)| next.renderer_id != current.renderer_id)
        {
            return Err(StoreError::InvalidState);
        }
        if snapshots
            .iter()
            .any(|(snapshot, _)| review.assets.contains_key(&snapshot.id))
        {
            return Err(StoreError::InvalidState);
        }
        let number = previous.number + 1;
        let mut stored = Vec::with_capacity(snapshots.len());
        for (position, (mut snapshot, bytes)) in snapshots.into_iter().enumerate() {
            snapshot.review_id = review_id.into();
            snapshot.position = position as i32;
            snapshot.asset_path = snapshot.id.clone();
            review.assets.insert(snapshot.id.clone(), bytes);
            stored.push(snapshot);
        }
        review.revisions.push(RevisionRecord {
            number,
            created_at: Utc::now().to_rfc3339(),
            snapshots: stored,
        });
        review.state = ReviewState::Pending;
        review.result = None;
        review.updating_at = None;
        Self::notify(review);
        Ok(number)
    }

    pub fn get_asset_bytes(
        &self,
        review_id: &str,
        snapshot_id: &str,
    ) -> Result<Vec<u8>, StoreError> {
        let state = self.state.lock();
        let review = state.reviews.get(review_id).ok_or(StoreError::NotFound)?;
        review
            .assets
            .get(snapshot_id)
            .cloned()
            .ok_or(StoreError::NotFound)
    }

    pub fn subscribe(&self, review_id: &str) -> Result<watch::Receiver<u64>, StoreError> {
        let state = self.state.lock();
        let review = state.reviews.get(review_id).ok_or(StoreError::NotFound)?;
        Ok(review.notify.subscribe())
    }

    pub fn validate_token(&self, review_id: &str, token: &str) -> Result<bool, StoreError> {
        let state = self.state.lock();
        let review = state.reviews.get(review_id).ok_or(StoreError::NotFound)?;
        Ok(review.token_hash == Self::hash_token(token))
    }

    fn notify(review: &ReviewRecord) {
        let next = review.notify.borrow().wrapping_add(1);
        review.notify.send_replace(next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(id: &str, renderer: &str) -> StoredSnapshot {
        StoredSnapshot {
            id: id.into(),
            review_id: String::new(),
            position: 0,
            renderer_id: renderer.into(),
            mapping_version: 1,
            width: 100,
            height: 100,
            mime_type: "image/png".into(),
            asset_path: String::new(),
            source_label: "example".into(),
            mapping_json: "{}".into(),
        }
    }

    fn generation() -> Generation {
        Generation {
            variants_enabled: false,
            count: 1,
            guides: vec![],
        }
    }

    #[test]
    fn revisions_preserve_original_and_are_immutable() {
        let store = ReviewStore::new();
        let id = store
            .create_review(
                None,
                vec![(snapshot("old", "image"), b"before".to_vec())],
                store.generate_token(),
            )
            .unwrap();
        assert!(matches!(
            store.append_revision(&id, vec![(snapshot("new", "image"), b"after".to_vec())]),
            Err(StoreError::InvalidState)
        ));
        store.submit_review(&id, vec![], generation()).unwrap();
        let provisional = store.list_revisions(&id).unwrap();
        assert_eq!(provisional.len(), 2);
        assert!(provisional[1].snapshots.is_empty());
        assert!(store.is_updating(&id).unwrap());
        assert!(matches!(
            store.append_revision(&id, vec![(snapshot("wrong", "code"), vec![])]),
            Err(StoreError::InvalidState)
        ));
        assert_eq!(
            store
                .append_revision(&id, vec![(snapshot("new", "image"), b"after".to_vec())])
                .unwrap(),
            2
        );
        assert_eq!(store.get_review_state(&id).unwrap().status, "pending");
        assert!(!store.is_updating(&id).unwrap());
        assert_eq!(store.list_feedback(&id).unwrap().len(), 1);
        store.submit_review(&id, vec![], generation()).unwrap();
        assert_eq!(store.list_feedback(&id).unwrap().len(), 2);
        assert_eq!(
            store
                .append_revision(&id, vec![(snapshot("third", "image"), b"latest".to_vec())])
                .unwrap(),
            3
        );
        let revisions = store.list_revisions(&id).unwrap();
        assert_eq!(
            revisions
                .iter()
                .map(|revision| revision.number)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(revisions[0].snapshots[0].id, "old");
        assert_eq!(revisions[2].snapshots[0].id, "third");
        assert_eq!(store.get_asset_bytes(&id, "old").unwrap(), b"before");
        assert_eq!(store.get_asset_bytes(&id, "new").unwrap(), b"after");
        assert_eq!(store.get_asset_bytes(&id, "third").unwrap(), b"latest");
    }

    #[test]
    fn questions_accept_one_reply_and_updates_can_be_aborted() {
        let store = ReviewStore::new();
        let id = store
            .create_review(
                None,
                vec![(snapshot("first", "image"), vec![1])],
                store.generate_token(),
            )
            .unwrap();
        store.submit_review(&id, vec![], generation()).unwrap();
        let question = store.add_message(&id, "question", "Which one?").unwrap();
        assert_eq!(store.get_reply(&id, &question).unwrap(), None);
        store.reply_message(&id, &question, "The second").unwrap();
        assert!(store.reply_message(&id, &question, "Again").is_err());
        assert_eq!(
            store.get_reply(&id, &question).unwrap().as_deref(),
            Some("The second")
        );
        store.begin_update(&id).unwrap();
        assert!(store.is_updating(&id).unwrap());
        store.abort_update(&id).unwrap();
        assert!(!store.is_updating(&id).unwrap());
    }

    #[test]
    fn progress_messages_are_short_single_lines_without_terminal_periods() {
        let store = ReviewStore::new();
        let id = store
            .create_review(
                None,
                vec![(snapshot("first", "image"), vec![1])],
                store.generate_token(),
            )
            .unwrap();
        assert!(store
            .add_message(&id, "progress", "Rendering updated UI")
            .is_ok());
        assert!(store.add_message(&id, "progress", "Finished.").is_err());
        assert!(store.add_message(&id, "progress", "1行目\n2行目").is_err());
        assert!(store.add_message(&id, "progress", &"a".repeat(61)).is_err());
    }

    #[test]
    fn revision_events_are_isolated_and_cancel_clears_updates() {
        let store = ReviewStore::new();
        let first = store
            .create_review(
                None,
                vec![(snapshot("first", "image"), vec![1])],
                store.generate_token(),
            )
            .unwrap();
        let second = store
            .create_review(
                None,
                vec![(snapshot("other", "image"), vec![2])],
                store.generate_token(),
            )
            .unwrap();
        let mut first_events = store.subscribe(&first).unwrap();
        let second_events = store.subscribe(&second).unwrap();
        store.submit_review(&first, vec![], generation()).unwrap();
        assert!(first_events.has_changed().unwrap());
        assert!(!second_events.has_changed().unwrap());
        first_events.borrow_and_update();
        store
            .append_revision(&first, vec![(snapshot("next", "image"), vec![3])])
            .unwrap();
        assert!(first_events.has_changed().unwrap());

        store.begin_update(&second).unwrap();
        store.cancel_review(&second).unwrap();
        assert!(!store.is_updating(&second).unwrap());
        assert_eq!(store.list_revisions(&second).unwrap().len(), 1);
    }
}
