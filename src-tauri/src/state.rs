//! Shared application state. The Data-Encryption-Key lives here only while the
//! vault is unlocked and is zeroized the moment it is cleared (lock / close).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Mutex;

use rusqlite::Connection;
use zeroize::Zeroizing;

use crate::crypto::KEY_LEN;

/// The DEK pinned in RAM. The key is boxed so its address is stable, then the
/// page is locked (best-effort `VirtualLock`/`mlock`) so it can't be paged out
/// to swap/hibernation on disk. On drop the bytes are zeroized and then the
/// page is unlocked (field drop order: `key` before `_lock`).
struct LockedKey {
    key: Box<Zeroizing<[u8; KEY_LEN]>>,
    /// `None` if the OS refused to lock (e.g. RLIMIT_MEMLOCK); the key still
    /// works, it just isn't swap-protected. Held only to unlock on drop.
    _lock: Option<region::LockGuard>,
}

impl LockedKey {
    fn new(dek: Zeroizing<[u8; KEY_LEN]>) -> Self {
        let key = Box::new(dek);
        // Safety: pointer + len describe exactly the boxed key's storage, which
        // stays alive (and at a fixed address) for as long as the guard.
        let lock = region::lock(key.as_ptr(), KEY_LEN).ok();
        LockedKey { key, _lock: lock }
    }
}

pub struct AppState {
    pub conn: Mutex<Connection>,
    /// Present only while unlocked.
    dek: Mutex<Option<LockedKey>>,
    /// Origins (registrable domain) the user has approved for browser autofill in
    /// the CURRENT unlocked session. Cleared on lock so consent is re-confirmed
    /// after every relock — a persisted allowlist would let local malware fill
    /// approved origins silently forever (the exact M-A threat).
    approved_origins: Mutex<HashSet<String>>,
    /// In-flight autofill consent prompts: request id -> reply channel. The bridge
    /// thread parks on the receiver while the main window asks the user.
    consents: Mutex<HashMap<u64, Sender<bool>>>,
    consent_seq: AtomicU64,
}

impl AppState {
    pub fn new(conn: Connection) -> Self {
        AppState {
            conn: Mutex::new(conn),
            dek: Mutex::new(None),
            approved_origins: Mutex::new(HashSet::new()),
            consents: Mutex::new(HashMap::new()),
            consent_seq: AtomicU64::new(1),
        }
    }

    pub fn set_dek(&self, dek: Zeroizing<[u8; KEY_LEN]>) {
        let mut guard = self.dek.lock().expect("dek mutex poisoned");
        *guard = Some(LockedKey::new(dek)); // old value (if any) drops -> zeroized + unlocked
    }

    /// Drop and zeroize the in-memory key. Also forgets every autofill approval and
    /// cancels pending consent prompts (dropping the senders => the bridge sees a
    /// denied/closed channel) so a relock fully resets the autofill trust state.
    pub fn clear_dek(&self) {
        let mut guard = self.dek.lock().expect("dek mutex poisoned");
        *guard = None;
        self.approved_origins
            .lock()
            .expect("approved mutex poisoned")
            .clear();
        self.consents
            .lock()
            .expect("consents mutex poisoned")
            .clear();
    }

    /// Has the user already approved this origin key for autofill this session?
    pub fn is_origin_approved(&self, key: &str) -> bool {
        self.approved_origins
            .lock()
            .expect("approved mutex poisoned")
            .contains(key)
    }

    /// Remember an approved origin key for the rest of the unlocked session.
    pub fn approve_origin(&self, key: String) {
        self.approved_origins
            .lock()
            .expect("approved mutex poisoned")
            .insert(key);
    }

    /// Register a pending consent prompt; returns its id + the receiver the bridge
    /// thread blocks on until `resolve_consent` (or a relock) answers it.
    pub fn register_consent(&self) -> (u64, std::sync::mpsc::Receiver<bool>) {
        let id = self.consent_seq.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = std::sync::mpsc::channel();
        self.consents
            .lock()
            .expect("consents mutex poisoned")
            .insert(id, tx);
        (id, rx)
    }

    /// Deliver the user's decision to the waiting bridge thread.
    pub fn resolve_consent(&self, id: u64, approved: bool) {
        if let Some(tx) = self
            .consents
            .lock()
            .expect("consents mutex poisoned")
            .remove(&id)
        {
            let _ = tx.send(approved);
        }
    }

    pub fn is_unlocked(&self) -> bool {
        self.dek.lock().expect("dek mutex poisoned").is_some()
    }

    /// Run `f` with a reference to the live DEK, or return `Locked` if not unlocked.
    pub fn with_dek<T>(
        &self,
        f: impl FnOnce(&[u8; KEY_LEN]) -> crate::error::AppResult<T>,
    ) -> crate::error::AppResult<T> {
        let guard = self.dek.lock().expect("dek mutex poisoned");
        match guard.as_ref() {
            Some(locked) => f(&locked.key),
            None => Err(crate::error::AppError::Locked),
        }
    }
}
