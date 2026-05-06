use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex, RwLock, Weak,
};

use thiserror::Error;

type Reducer<S, A> = dyn Fn(&mut S, A) + Send + Sync + 'static;
type WatchCallback<S> = dyn Fn(Arc<S>) + Send + Sync + 'static;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RoomError {
    #[error("Room configuration error: {0}")]
    Configuration(String),
    #[error("Room middleware rejected action: {0}")]
    Middleware(String),
    #[error("Room synchronization failure: {0}")]
    Synchronization(&'static str),
    #[error("Transaction has already been committed")]
    TransactionCommitted,
}

pub trait Middleware<S, A>: Send + Sync {
    fn before_action(&self, _state: &S, _action: &A) -> Result<(), RoomError> {
        Ok(())
    }

    fn after_commit(&self, _old_state: &S, _new_state: &S, _actions: &[A]) {}
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum WatchScope {
    #[default]
    All,
}

#[derive(Clone)]
struct Watcher<S> {
    scope: WatchScope,
    callback: Arc<WatchCallback<S>>,
}

pub struct SubscriptionHandle {
    cleanup: Option<Box<dyn FnOnce() + Send + Sync>>,
}

impl SubscriptionHandle {
    pub fn unsubscribe(mut self) {
        if let Some(cleanup) = self.cleanup.take() {
            cleanup();
        }
    }
}

impl Drop for SubscriptionHandle {
    fn drop(&mut self) {
        if let Some(cleanup) = self.cleanup.take() {
            cleanup();
        }
    }
}

#[derive(Clone, Default)]
pub struct LoggingMiddleware;

impl<S, A> Middleware<S, A> for LoggingMiddleware {
    fn after_commit(&self, _old_state: &S, _new_state: &S, actions: &[A]) {
        crate::LOGGER.debug(&format!(
            "Room transaction committed with {} action(s)",
            actions.len()
        ));
    }
}

struct RoomServiceInner<S, A>
where
    S: Clone + Send + Sync + 'static,
    A: Clone + Send + Sync + 'static,
{
    state: RwLock<Arc<S>>,
    reducer: Arc<Reducer<S, A>>,
    middlewares: Vec<Arc<dyn Middleware<S, A>>>,
    watchers: Arc<Mutex<HashMap<usize, Watcher<S>>>>,
    next_watcher_id: AtomicUsize,
}

impl<S, A> RoomServiceInner<S, A>
where
    S: Clone + Send + Sync + 'static,
    A: Clone + Send + Sync + 'static,
{
    fn get_state(&self) -> Result<Arc<S>, RoomError> {
        self.state
            .read()
            .map(|guard| guard.clone())
            .map_err(|_| RoomError::Synchronization("state read lock poisoned"))
    }

    fn collect_watchers(&self, scope: WatchScope) -> Result<Vec<Arc<WatchCallback<S>>>, RoomError> {
        let watchers = self
            .watchers
            .lock()
            .map_err(|_| RoomError::Synchronization("watchers lock poisoned"))?;

        Ok(watchers
            .values()
            .filter(|watcher| watcher.scope == scope)
            .map(|watcher| watcher.callback.clone())
            .collect())
    }
}

pub trait Room: Send + Sync {
    type State: Clone + Send + Sync + 'static;
    type Action: Clone + Send + Sync + 'static;

    fn get_state(&self) -> Arc<Self::State>;
    fn dispatch(&self, action: Self::Action) -> Result<(), RoomError>;
    fn begin_transaction(&self) -> Result<Transaction<Self::State, Self::Action>, RoomError>;

    fn watch_all<F>(&self, callback: F) -> Result<SubscriptionHandle, RoomError>
    where
        F: Fn(Arc<Self::State>) + Send + Sync + 'static;
}

#[derive(Clone)]
pub struct RoomService<S, A>
where
    S: Clone + Send + Sync + 'static,
    A: Clone + Send + Sync + 'static,
{
    inner: Arc<RoomServiceInner<S, A>>,
}

impl<S, A> RoomService<S, A>
where
    S: Clone + Send + Sync + 'static,
    A: Clone + Send + Sync + 'static,
{
    fn new(
        initial_state: S,
        reducer: Arc<Reducer<S, A>>,
        middlewares: Vec<Arc<dyn Middleware<S, A>>>,
    ) -> Self {
        Self {
            inner: Arc::new(RoomServiceInner {
                state: RwLock::new(Arc::new(initial_state)),
                reducer,
                middlewares,
                watchers: Arc::new(Mutex::new(HashMap::new())),
                next_watcher_id: AtomicUsize::new(1),
            }),
        }
    }
}

impl<S, A> Room for RoomService<S, A>
where
    S: Clone + Send + Sync + 'static,
    A: Clone + Send + Sync + 'static,
{
    type State = S;
    type Action = A;

    fn get_state(&self) -> Arc<Self::State> {
        self.inner
            .get_state()
            .expect("room state lock poisoned while reading")
    }

    fn dispatch(&self, action: Self::Action) -> Result<(), RoomError> {
        let mut transaction = self.begin_transaction()?;
        transaction.dispatch(action)?;
        transaction.commit()
    }

    fn begin_transaction(&self) -> Result<Transaction<Self::State, Self::Action>, RoomError> {
        let state = self.inner.get_state()?;

        Ok(Transaction {
            inner: self.inner.clone(),
            working_state: (*state).clone(),
            actions: Vec::new(),
            committed: false,
        })
    }

    fn watch_all<F>(&self, callback: F) -> Result<SubscriptionHandle, RoomError>
    where
        F: Fn(Arc<Self::State>) + Send + Sync + 'static,
    {
        let id = self.inner.next_watcher_id.fetch_add(1, Ordering::Relaxed);
        let watchers = self.inner.watchers.clone();

        watchers
            .lock()
            .map_err(|_| RoomError::Synchronization("watchers lock poisoned"))?
            .insert(
                id,
                Watcher {
                    scope: WatchScope::All,
                    callback: Arc::new(callback),
                },
            );

        let weak_watchers: Weak<Mutex<HashMap<usize, Watcher<S>>>> = Arc::downgrade(&watchers);

        Ok(SubscriptionHandle {
            cleanup: Some(Box::new(move || {
                if let Some(watchers) = weak_watchers.upgrade() {
                    if let Ok(mut watchers) = watchers.lock() {
                        watchers.remove(&id);
                    }
                }
            })),
        })
    }
}

pub struct Transaction<S, A>
where
    S: Clone + Send + Sync + 'static,
    A: Clone + Send + Sync + 'static,
{
    inner: Arc<RoomServiceInner<S, A>>,
    working_state: S,
    actions: Vec<A>,
    committed: bool,
}

impl<S, A> Transaction<S, A>
where
    S: Clone + Send + Sync + 'static,
    A: Clone + Send + Sync + 'static,
{
    pub fn dispatch(&mut self, action: A) -> Result<(), RoomError> {
        if self.committed {
            return Err(RoomError::TransactionCommitted);
        }

        for middleware in &self.inner.middlewares {
            middleware.before_action(&self.working_state, &action)?;
        }

        (self.inner.reducer)(&mut self.working_state, action.clone());
        self.actions.push(action);
        Ok(())
    }

    pub fn commit(mut self) -> Result<(), RoomError> {
        if self.committed {
            return Err(RoomError::TransactionCommitted);
        }

        if self.actions.is_empty() {
            self.committed = true;
            return Ok(());
        }

        let old_state = self.inner.get_state()?;
        let new_state = Arc::new(self.working_state.clone());

        {
            let mut state = self
                .inner
                .state
                .write()
                .map_err(|_| RoomError::Synchronization("state write lock poisoned"))?;
            *state = new_state.clone();
        }

        for middleware in &self.inner.middlewares {
            middleware.after_commit(old_state.as_ref(), new_state.as_ref(), &self.actions);
        }

        let watchers = self.inner.collect_watchers(WatchScope::All)?;
        for watcher in watchers {
            watcher(new_state.clone());
        }

        self.committed = true;
        Ok(())
    }
}

pub struct RoomServiceBuilder<S, A>
where
    S: Clone + Send + Sync + 'static,
    A: Clone + Send + Sync + 'static,
{
    initial_state: Option<S>,
    reducer: Option<Arc<Reducer<S, A>>>,
    middlewares: Vec<Arc<dyn Middleware<S, A>>>,
}

impl<S, A> Default for RoomServiceBuilder<S, A>
where
    S: Clone + Send + Sync + 'static,
    A: Clone + Send + Sync + 'static,
{
    fn default() -> Self {
        Self {
            initial_state: None,
            reducer: None,
            middlewares: Vec::new(),
        }
    }
}

impl<S, A> RoomServiceBuilder<S, A>
where
    S: Clone + Send + Sync + 'static,
    A: Clone + Send + Sync + 'static,
{
    pub fn new() -> Self {
        Self::default()
    }

    pub fn initial_state(mut self, initial_state: S) -> Self {
        self.initial_state = Some(initial_state);
        self
    }

    pub fn reducer<F>(mut self, reducer: F) -> Self
    where
        F: Fn(&mut S, A) + Send + Sync + 'static,
    {
        self.reducer = Some(Arc::new(reducer));
        self
    }

    pub fn middleware<M>(mut self, middleware: M) -> Self
    where
        M: Middleware<S, A> + 'static,
    {
        self.middlewares.push(Arc::new(middleware));
        self
    }

    pub fn build(self) -> Result<RoomService<S, A>, RoomError> {
        let initial_state = self
            .initial_state
            .ok_or_else(|| RoomError::Configuration("missing initial state".to_string()))?;
        let reducer = self
            .reducer
            .ok_or_else(|| RoomError::Configuration("missing reducer".to_string()))?;

        Ok(RoomService::new(initial_state, reducer, self.middlewares))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    #[derive(Clone)]
    struct RejectEvenMiddleware;

    impl Middleware<i32, i32> for RejectEvenMiddleware {
        fn before_action(&self, _state: &i32, action: &i32) -> Result<(), RoomError> {
            if action % 2 == 0 {
                Err(RoomError::Middleware(
                    "even actions are rejected".to_string(),
                ))
            } else {
                Ok(())
            }
        }
    }

    #[derive(Clone)]
    struct AfterCommitProbe {
        called: Arc<AtomicBool>,
        actions_seen: Arc<Mutex<Vec<i32>>>,
        old_state: Arc<Mutex<Option<i32>>>,
        new_state: Arc<Mutex<Option<i32>>>,
    }

    impl Middleware<i32, i32> for AfterCommitProbe {
        fn after_commit(&self, old_state: &i32, new_state: &i32, actions: &[i32]) {
            self.called.store(true, Ordering::SeqCst);
            self.actions_seen
                .lock()
                .expect("actions_seen lock should be available")
                .extend_from_slice(actions);
            *self
                .old_state
                .lock()
                .expect("old_state lock should be available") = Some(*old_state);
            *self
                .new_state
                .lock()
                .expect("new_state lock should be available") = Some(*new_state);
        }
    }

    fn build_room() -> RoomService<i32, i32> {
        RoomServiceBuilder::new()
            .initial_state(0)
            .reducer(|state, action| *state += action)
            .build()
            .expect("room should build")
    }

    #[test]
    fn dispatch_updates_state_and_notifies_once() {
        let room = build_room();
        let notifications = Arc::new(AtomicUsize::new(0));
        let latest_value = Arc::new(Mutex::new(None));

        let notification_counter = notifications.clone();
        let latest_value_ref = latest_value.clone();
        let _subscription = room
            .watch_all(move |state| {
                notification_counter.fetch_add(1, Ordering::SeqCst);
                *latest_value_ref
                    .lock()
                    .expect("latest_value lock should be available") = Some(*state);
            })
            .expect("subscription should register");

        room.dispatch(3).expect("dispatch should succeed");

        assert_eq!(*room.get_state(), 3);
        assert_eq!(notifications.load(Ordering::SeqCst), 1);
        assert_eq!(
            *latest_value
                .lock()
                .expect("latest_value lock should be available"),
            Some(3)
        );
    }

    #[test]
    fn transaction_commits_atomically_with_single_notification() {
        let room = build_room();
        let notifications = Arc::new(AtomicUsize::new(0));

        let notification_counter = notifications.clone();
        let _subscription = room
            .watch_all(move |_| {
                notification_counter.fetch_add(1, Ordering::SeqCst);
            })
            .expect("subscription should register");

        let mut transaction = room.begin_transaction().expect("transaction should begin");
        transaction
            .dispatch(1)
            .expect("first action should succeed");
        transaction
            .dispatch(2)
            .expect("second action should succeed");
        transaction.commit().expect("commit should succeed");

        assert_eq!(*room.get_state(), 3);
        assert_eq!(notifications.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn dropped_transaction_rolls_back() {
        let room = build_room();

        {
            let mut transaction = room.begin_transaction().expect("transaction should begin");
            transaction.dispatch(5).expect("action should succeed");
        }

        assert_eq!(*room.get_state(), 0);
    }

    #[test]
    fn middleware_veto_preserves_prior_snapshot() {
        let room = RoomServiceBuilder::new()
            .initial_state(1)
            .reducer(|state, action| *state += action)
            .middleware(RejectEvenMiddleware)
            .build()
            .expect("room should build");

        let error = room
            .dispatch(2)
            .expect_err("middleware should reject even action");

        assert_eq!(
            error,
            RoomError::Middleware("even actions are rejected".to_string())
        );
        assert_eq!(*room.get_state(), 1);
    }

    #[test]
    fn middleware_after_commit_receives_old_new_and_actions() {
        let probe = AfterCommitProbe {
            called: Arc::new(AtomicBool::new(false)),
            actions_seen: Arc::new(Mutex::new(Vec::new())),
            old_state: Arc::new(Mutex::new(None)),
            new_state: Arc::new(Mutex::new(None)),
        };

        let room = RoomServiceBuilder::new()
            .initial_state(10)
            .reducer(|state, action| *state += action)
            .middleware(probe.clone())
            .build()
            .expect("room should build");

        let mut transaction = room.begin_transaction().expect("transaction should begin");
        transaction
            .dispatch(3)
            .expect("first action should succeed");
        transaction
            .dispatch(4)
            .expect("second action should succeed");
        transaction.commit().expect("commit should succeed");

        assert!(probe.called.load(Ordering::SeqCst));
        assert_eq!(
            *probe
                .actions_seen
                .lock()
                .expect("actions_seen lock should be available"),
            vec![3, 4]
        );
        assert_eq!(
            *probe
                .old_state
                .lock()
                .expect("old_state lock should be available"),
            Some(10)
        );
        assert_eq!(
            *probe
                .new_state
                .lock()
                .expect("new_state lock should be available"),
            Some(17)
        );
    }

    #[test]
    fn subscription_handle_unsubscribes_on_drop() {
        let room = build_room();
        let notifications = Arc::new(AtomicUsize::new(0));

        {
            let notification_counter = notifications.clone();
            let _subscription = room
                .watch_all(move |_| {
                    notification_counter.fetch_add(1, Ordering::SeqCst);
                })
                .expect("subscription should register");

            room.dispatch(1).expect("dispatch should succeed");
        }

        room.dispatch(1).expect("dispatch should succeed");

        assert_eq!(notifications.load(Ordering::SeqCst), 1);
    }
}
