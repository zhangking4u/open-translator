//! Single-flight job slot with latest-wins follow-up handling.
//!
//! One job runs at a time; jobs submitted while one is running are not queued
//! in order — the newest one replaces any previous follow-up. The selection
//! card and the caption translation line both need this: the engine is
//! serialized, and a burst of watcher triggers must coalesce into at most one
//! follow-up so stale streams never keep rewriting the UI after it moved on.

use std::sync::Mutex;

/// Serializes jobs and remembers at most one follow-up: the latest.
///
/// The caller owns execution. [`Self::submit`] returns the item to run when
/// the slot was idle; [`Self::finish`] returns the follow-up to run next when
/// one arrived while the job was running. This stays a plain state machine
/// (no threads, timers or callbacks), so both callers keep their own spawn
/// logic and the race behaviour is unit-testable.
pub struct LatestWins<T> {
    state: Mutex<State<T>>,
}

enum State<T> {
    Idle,
    Running(Option<T>),
}

impl<T> Default for LatestWins<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> LatestWins<T> {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State::Idle),
        }
    }

    /// Submits `item`. Returns `Some(item)` when it must run now (the slot was
    /// idle and is now marked running); returns `None` when a job is already
    /// running, in which case `item` replaces any pending follow-up.
    pub fn submit(&self, item: T) -> Option<T> {
        let mut state = self.state.lock().unwrap();

        match &mut *state {
            State::Idle => {
                *state = State::Running(None);
                Some(item)
            }
            State::Running(queued) => {
                *queued = Some(item);
                None
            }
        }
    }

    /// Marks the current job as finished. Returns the follow-up to run next
    /// (the slot stays running while the caller executes it), or `None` when
    /// no follow-up arrived and the slot is idle again.
    ///
    /// The take and the idle transition happen under one lock, so a
    /// submission racing this call is either seen as the follow-up or starts a
    /// fresh run — it can never be lost.
    pub fn finish(&self) -> Option<T> {
        let mut state = self.state.lock().unwrap();

        match &mut *state {
            State::Running(queued) => match queued.take() {
                Some(next) => Some(next),
                None => {
                    *state = State::Idle;
                    None
                }
            },
            State::Idle => None,
        }
    }

    /// Drops any pending follow-up without touching the running job (used
    /// when a feature is switched off mid-translation).
    pub fn clear_queued(&self) {
        let mut state = self.state.lock().unwrap();

        if let State::Running(queued) = &mut *state {
            *queued = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LatestWins;

    #[test]
    fn runs_immediately_when_idle() {
        let slot = LatestWins::<&str>::new();

        assert_eq!(slot.submit("first"), Some("first"));
    }

    #[test]
    fn keeps_only_the_newest_follow_up() {
        let slot = LatestWins::<&str>::new();

        assert_eq!(slot.submit("running"), Some("running"));
        assert_eq!(slot.submit("old"), None);
        assert_eq!(slot.submit("newest"), None);
        assert_eq!(slot.finish(), Some("newest"));
        assert_eq!(slot.finish(), None);
    }

    #[test]
    fn returns_to_idle_after_finish() {
        let slot = LatestWins::<u32>::new();

        assert_eq!(slot.submit(1), Some(1));
        assert_eq!(slot.finish(), None);
        assert_eq!(slot.submit(2), Some(2));
    }

    #[test]
    fn finish_on_idle_is_none() {
        let slot = LatestWins::<u32>::new();

        assert_eq!(slot.finish(), None);
        assert_eq!(slot.submit(7), Some(7));
    }

    #[test]
    fn chains_through_each_follow_up() {
        let slot = LatestWins::<u32>::new();

        assert_eq!(slot.submit(1), Some(1));
        assert_eq!(slot.submit(2), None);

        // The follow-up keeps the slot running; the next finish is idle again.
        assert_eq!(slot.finish(), Some(2));
        assert_eq!(slot.finish(), None);
    }

    #[test]
    fn clear_queued_keeps_the_running_job() {
        let slot = LatestWins::<u32>::new();

        assert_eq!(slot.submit(1), Some(1));
        assert_eq!(slot.submit(2), None);
        slot.clear_queued();

        assert_eq!(slot.finish(), None);
        assert_eq!(slot.submit(3), Some(3));
    }

    /// Many submitters race one shared slot; whoever wins `submit` runs the
    /// job and drains follow-ups. Two jobs must never overlap and the slot
    /// must not be left stuck when the producers stop.
    #[test]
    fn concurrent_submit_and_finish_never_stack_jobs() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Barrier};

        const PRODUCERS: usize = 4;
        const ROUNDS: usize = 250;

        let slot = Arc::new(LatestWins::<usize>::new());
        let in_flight = Arc::new(AtomicUsize::new(0));
        let processed = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(PRODUCERS + 1));

        let mut handles = Vec::new();

        for producer in 0..PRODUCERS {
            let slot = Arc::clone(&slot);
            let in_flight = Arc::clone(&in_flight);
            let processed = Arc::clone(&processed);
            let barrier = Arc::clone(&barrier);

            handles.push(std::thread::spawn(move || {
                barrier.wait();

                for round in 0..ROUNDS {
                    let mut current = slot.submit(producer * ROUNDS + round);

                    while let Some(item) = current {
                        let before = in_flight.fetch_add(1, Ordering::SeqCst);
                        assert_eq!(before, 0, "two jobs ran at the same time");

                        std::thread::yield_now();

                        in_flight.fetch_sub(1, Ordering::SeqCst);
                        processed.fetch_add(1, Ordering::SeqCst);

                        let _ = item;
                        current = slot.finish();
                    }
                }
            }));
        }

        barrier.wait();

        for handle in handles {
            handle.join().unwrap();
        }

        assert!(processed.load(Ordering::SeqCst) > 0, "no job ran at all");
        assert!(
            slot.submit(usize::MAX).is_some(),
            "the slot must be idle after all producers stopped"
        );
    }
}
