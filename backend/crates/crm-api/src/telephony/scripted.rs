//! `ScriptedProvider` (docs/specs/SLICE_006.md §3, §14 item 7): a
//! `TelephonyProvider` for tests — a queue of dial outcomes/errors, a
//! presence table, a create-room failure switch, and a record of every
//! call. `pub` behind the `test-support` feature, as the Operator's is.
//! A `Blocked` dial parks until the test releases it through a oneshot —
//! the "hangup while the dial is in flight" case (§13 item 2).

use std::collections::{HashSet, VecDeque};
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::oneshot;

use super::{DialOutcome, DialRequest, PhoneNumber, ProviderError, TelephonyProvider};

/// One queued answer to `dial`.
pub enum ScriptedDial {
    Resolve(Result<DialOutcome, ProviderError>),
    /// Parks until the sender resolves it (or is dropped → `Unavailable`).
    Blocked(oneshot::Receiver<Result<DialOutcome, ProviderError>>),
}

/// Every provider call, in order. `to_number` keeps its redacting type so
/// a `{:?}` of the record can never print the fixture number; tests call
/// `expose()` deliberately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordedCall {
    CreateRoom {
        room: String,
        max_call: Duration,
    },
    ParticipantPresent {
        room: String,
        identity: String,
    },
    Dial {
        room: String,
        to_number: PhoneNumber,
        participant_identity: String,
        ring_timeout: Duration,
        max_call: Duration,
    },
    Hangup {
        room: String,
    },
}

#[derive(Default)]
struct Inner {
    dials: VecDeque<ScriptedDial>,
    create_room_failure: Option<ProviderError>,
    present: HashSet<(String, String)>,
    calls: Vec<RecordedCall>,
    dials_completed: usize,
    callee_leaves_immediately: bool,
}

pub struct ScriptedProvider {
    inner: Mutex<Inner>,
    dial_done: tokio::sync::Notify,
}

impl Default for ScriptedProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptedProvider {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            dial_done: tokio::sync::Notify::new(),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Queues the next `dial` result.
    pub fn push_dial(&self, result: Result<DialOutcome, ProviderError>) {
        self.lock().dials.push_back(ScriptedDial::Resolve(result));
    }

    /// Queues a parked `dial`; the returned sender releases it.
    pub fn push_blocked_dial(&self) -> oneshot::Sender<Result<DialOutcome, ProviderError>> {
        let (tx, rx) = oneshot::channel();
        self.lock().dials.push_back(ScriptedDial::Blocked(rx));
        tx
    }

    /// Makes every subsequent `create_room` fail with `err`.
    pub fn fail_create_room(&self, err: ProviderError) {
        self.lock().create_room_failure = Some(err);
    }

    /// When set, an `Answered` dial does *not* mark the SIP participant
    /// present — the sub-second answer-and-hangup race the dial task's
    /// one re-check exists for (docs/specs/SLICE_006.md §2).
    pub fn set_callee_leaves_immediately(&self, leaves: bool) {
        self.lock().callee_leaves_immediately = leaves;
    }

    pub fn set_present(&self, room: &str, identity: &str, present: bool) {
        let key = (room.to_string(), identity.to_string());
        let mut inner = self.lock();
        if present {
            inner.present.insert(key);
        } else {
            inner.present.remove(&key);
        }
    }

    pub fn calls(&self) -> Vec<RecordedCall> {
        self.lock().calls.clone()
    }

    pub fn dials_completed(&self) -> usize {
        self.lock().dials_completed
    }

    /// Resolves once at least `n` dials have returned — how a test waits
    /// for the dial task's provider call to finish without a join handle.
    pub async fn wait_for_dials_completed(&self, n: usize) {
        loop {
            let notified = self.dial_done.notified();
            if self.dials_completed() >= n {
                return;
            }
            notified.await;
        }
    }
}

#[async_trait]
impl TelephonyProvider for ScriptedProvider {
    async fn create_room(&self, room: &str, max_call: Duration) -> Result<(), ProviderError> {
        let mut inner = self.lock();
        inner.calls.push(RecordedCall::CreateRoom {
            room: room.to_string(),
            max_call,
        });
        match &inner.create_room_failure {
            Some(err) => Err(err.clone()),
            None => Ok(()),
        }
    }

    async fn participant_present(&self, room: &str, identity: &str) -> Result<bool, ProviderError> {
        let mut inner = self.lock();
        inner.calls.push(RecordedCall::ParticipantPresent {
            room: room.to_string(),
            identity: identity.to_string(),
        });
        Ok(inner
            .present
            .contains(&(room.to_string(), identity.to_string())))
    }

    async fn dial(&self, req: DialRequest) -> Result<DialOutcome, ProviderError> {
        let next = {
            let mut inner = self.lock();
            inner.calls.push(RecordedCall::Dial {
                room: req.room.clone(),
                to_number: req.to_number.clone(),
                participant_identity: req.participant_identity.clone(),
                ring_timeout: req.ring_timeout,
                max_call: req.max_call,
            });
            inner.dials.pop_front()
        };
        let result = match next {
            None => Err(ProviderError::Unavailable(
                "scripted provider has no queued dial outcome".to_string(),
            )),
            Some(ScriptedDial::Resolve(result)) => result,
            Some(ScriptedDial::Blocked(rx)) => rx.await.unwrap_or_else(|_| {
                Err(ProviderError::Unavailable(
                    "scripted blocked dial dropped".to_string(),
                ))
            }),
        };
        {
            let mut inner = self.lock();
            // A real SIP participant is in the room once the callee answers
            // — the dial task's post-answer presence re-check sees it until
            // a test removes it or the room is hung up.
            if matches!(result, Ok(DialOutcome::Answered { .. }))
                && !inner.callee_leaves_immediately
            {
                inner
                    .present
                    .insert((req.room.clone(), req.participant_identity.clone()));
            }
            inner.dials_completed += 1;
        }
        self.dial_done.notify_waiters();
        result
    }

    async fn hangup(&self, room: &str) -> Result<(), ProviderError> {
        let mut inner = self.lock();
        inner.calls.push(RecordedCall::Hangup {
            room: room.to_string(),
        });
        inner.present.retain(|(r, _)| r != room);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telephony::SipFailure;

    fn req(room: &str) -> DialRequest {
        DialRequest {
            room: room.to_string(),
            to_number: PhoneNumber::new("+15555550100".into()),
            participant_identity: "sip:x".into(),
            ring_timeout: Duration::from_secs(45),
            max_call: Duration::from_secs(3600),
        }
    }

    #[tokio::test]
    async fn queue_drains_in_order_and_records_every_call() {
        let p = ScriptedProvider::new();
        p.push_dial(Ok(DialOutcome::Answered {
            call_ref: Some("ref".into()),
        }));
        p.push_dial(Ok(DialOutcome::Failed(SipFailure::Busy)));
        p.create_room("call:a", Duration::from_secs(10))
            .await
            .unwrap();
        assert!(!p.participant_present("call:a", "agent:u").await.unwrap());
        p.set_present("call:a", "agent:u", true);
        assert!(p.participant_present("call:a", "agent:u").await.unwrap());
        assert_eq!(
            p.dial(req("call:a")).await,
            Ok(DialOutcome::Answered {
                call_ref: Some("ref".into())
            })
        );
        // Answered marks the SIP participant present; hangup clears it.
        assert!(p.participant_present("call:a", "sip:x").await.unwrap());
        p.hangup("call:a").await.unwrap();
        assert!(!p.participant_present("call:a", "sip:x").await.unwrap());
        assert_eq!(
            p.dial(req("call:a")).await,
            Ok(DialOutcome::Failed(SipFailure::Busy))
        );
        assert!(matches!(
            p.dial(req("call:a")).await,
            Err(ProviderError::Unavailable(_))
        ));
        assert_eq!(p.dials_completed(), 3);
        let calls = p.calls();
        assert_eq!(calls.len(), 9);
        assert!(matches!(calls[0], RecordedCall::CreateRoom { .. }));
        let dial_debug = format!("{:?}", calls[3]);
        assert!(!dial_debug.contains("5550100"), "{dial_debug}");
    }

    #[tokio::test]
    async fn blocked_dial_waits_for_release_and_fails_on_drop() {
        let p = std::sync::Arc::new(ScriptedProvider::new());
        let tx = p.push_blocked_dial();
        let p2 = p.clone();
        let task = tokio::spawn(async move { p2.dial(req("call:b")).await });
        tokio::task::yield_now().await;
        assert_eq!(p.dials_completed(), 0);
        tx.send(Ok(DialOutcome::Failed(SipFailure::NoAnswer)))
            .unwrap();
        assert_eq!(
            task.await.unwrap(),
            Ok(DialOutcome::Failed(SipFailure::NoAnswer))
        );
        p.wait_for_dials_completed(1).await;

        let tx = p.push_blocked_dial();
        drop(tx);
        assert!(matches!(
            p.dial(req("call:b")).await,
            Err(ProviderError::Unavailable(_))
        ));
    }

    #[tokio::test]
    async fn create_room_failure_switch() {
        let p = ScriptedProvider::new();
        p.fail_create_room(ProviderError::Timeout);
        assert_eq!(
            p.create_room("call:c", Duration::from_secs(1)).await,
            Err(ProviderError::Timeout)
        );
    }
}
