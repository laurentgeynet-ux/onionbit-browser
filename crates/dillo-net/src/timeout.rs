//! Port de `src/timeout.cc` — timers one-shot.
//!
//! Amont : `Fl::add_timeout`/`Fl::repeat_timeout` arment un callback exécuté
//! **une fois** par la boucle principale FLTK. Ici : file de deadlines
//! drainée par [`poll_timeouts`] depuis le thread principal (single-writer,
//! comme [`crate::io::poll_io`]/[`crate::dns::poll_dns`]).

use std::cell::RefCell;
use std::time::{Duration, Instant};

/// Identifiant de timer (≙ la paire `{cb,data}` amont pour
/// `a_Timeout_actually_remove`).
pub type TimeoutId = u64;

type TimeoutCb = Box<dyn FnMut()>;

struct Pending {
    id: TimeoutId,
    deadline: Instant,
    cb: TimeoutCb,
}

struct Timers {
    next: TimeoutId,
    list: Vec<Pending>,
}

thread_local! {
    /// `a_Timeout_add` ≙ `Fl::add_timeout` — file thread-local.
    static TIMERS: RefCell<Timers> = const {
        RefCell::new(Timers {
            next: 1,
            list: Vec::new(),
        })
    };
}

fn add_lease(delay: f64, cb: TimeoutCb) -> TimeoutId {
    TIMERS.with(|t| {
        let mut t = t.borrow_mut();
        let id = t.next;
        t.next = t.next.wrapping_add(1).max(1);
        let secs = if delay.is_finite() && delay > 0.0 {
            delay
        } else {
            0.0
        };
        t.list.push(Pending {
            id,
            deadline: Instant::now() + Duration::from_secs_f64(secs),
            cb,
        });
        id
    })
}

/// `a_Timeout_add` — callback différé à `t` secondes (0 = plus tôt possible
/// depuis la boucle principale). One-shot.
pub fn timeout_add<F: FnMut() + 'static>(delay: f64, cb: F) -> TimeoutId {
    add_lease(delay, Box::new(cb))
}

/// `a_Timeout_repeat` — amont utilise `Fl::add_timeout` (identique à
/// `a_Timeout_add`, `timeout.cc:34-37`) : one-shot relatif à l'appel,
/// réarmé explicitement par le callback quand une périodicité est
/// voulue. Semblable ici — pas de drift à documenter.
pub fn timeout_repeat<F: FnMut() + 'static>(delay: f64, cb: F) -> TimeoutId {
    add_lease(delay, Box::new(cb))
}

/// `a_Timeout_remove` — **no-op amont** ("WTF? FIXME") : les timeouts FLTK
/// sont one-shot, il n'y a rien à retirer. Quirk porté tel quel.
pub const fn timeout_remove() {}

/// `a_Timeout_actually_remove` ≙ `Fl::remove_timeout` — amont matche par
/// `{cb, cbdata}` ; ici par id retourné par `add`.
pub fn actually_remove(id: TimeoutId) {
    TIMERS.with(|t| t.borrow_mut().list.retain(|p| p.id != id));
}

/// Drain des timers arrivés à échéance — équivalent de la passe FLTK.
/// Les callbacks peuvent armer de nouveaux timers (réentrants).
pub fn poll_timeouts() {
    loop {
        // Fidèle à FLTK : deadline la plus proche d'abord.
        let fired = TIMERS.with(|t| {
            let mut t = t.borrow_mut();
            let now = Instant::now();
            let (pos, _) = t
                .list
                .iter()
                .enumerate()
                .filter(|(_, p)| p.deadline <= now)
                .min_by_key(|(_, p)| p.deadline)?;
            Some(t.list.remove(pos))
        });
        match fired {
            Some(mut p) => (p.cb)(),
            None => return,
        }
    }
}

/// Nombre de timers armés (tests/diagnostic).
#[must_use]
pub fn pending_count() -> usize {
    TIMERS.with(|t| t.borrow().list.len())
}

/// Délai jusqu'au prochain timer — ≙ le `timeout` que `Fl::wait`
/// calcule pour bloquer la boucle ; utilisé pour armer
/// `ControlFlow::WaitUntil` quand des timers sont actifs.
#[must_use]
pub fn next_due() -> Option<Duration> {
    TIMERS.with(|t| {
        let now = Instant::now();
        t.borrow()
            .list
            .iter()
            .map(|p| p.deadline.saturating_duration_since(now))
            .min()
    })
}

/// `a_Timeout_remove` global au reset — vide la file (≙ `freeall` amont).
pub fn timeout_freeall() {
    TIMERS.with(|t| t.borrow_mut().list.clear());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn one_shot_fires_once() {
        timeout_freeall();
        let hits = Rc::new(Cell::new(0));
        let h = hits.clone();
        timeout_add(0.0, move || h.set(h.get() + 1));
        assert_eq!(pending_count(), 1);
        poll_timeouts();
        assert_eq!(hits.get(), 1);
        assert_eq!(pending_count(), 0);
        poll_timeouts(); // one-shot : ne refire pas
        assert_eq!(hits.get(), 1);
    }

    #[test]
    fn actually_remove_cancels() {
        timeout_freeall();
        let hits = Rc::new(Cell::new(0));
        let h = hits.clone();
        let id = timeout_add(0.0, move || h.set(h.get() + 1));
        actually_remove(id);
        poll_timeouts();
        assert_eq!(hits.get(), 0);
        timeout_remove(); // no-op amont : compilable, sans effet
    }

    #[test]
    fn callback_can_rearm() {
        // Chaque callback ré-arme le suivant tant que n < 3 — la file
        // nouvellement armée est drainée dans la même passe (≙ boucle
        // FLTK qui relit les timeouts arrivés à échéance).
        timeout_freeall();
        let n = Rc::new(Cell::new(0));
        let n2 = n.clone();
        timeout_add(0.0, move || {
            n2.set(n2.get() + 1);
            if n2.get() < 3 {
                let n3 = n2.clone();
                timeout_add(0.0, move || {
                    n3.set(n3.get() + 1);
                    if n3.get() < 3 {
                        let n4 = n3.clone();
                        timeout_add(0.0, move || n4.set(n4.get() + 1));
                    }
                });
            }
        });
        poll_timeouts();
        assert_eq!(n.get(), 3);
    }
}
