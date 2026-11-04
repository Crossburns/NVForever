//! What a resident does after furniture refused it (docs/spec-notes/furniture.md §7,
//! docs/design/actor-behaviour.md, "Furniture"). The rules only, without the engine: the
//! engine (`engine/nvforever/src/furniture.rs`) says why it refused, the resident
//! (`engine/nvforever/src/residents.rs`) holds the state and gives the orders.
//!
//! All of this is [design], an NVForever choice; nothing here is claimed to match the
//! original game. Occupied furniture may come free, so the actor asks again after a wait;
//! any other refusal repeats for the same request, so it does not ask again until
//! something it depends on has changed.

/// How long an actor refused because the furniture is occupied waits before it asks again:
/// seconds of active play in which it could sit, not counting menus, conversations or
/// loading screens [design: an engineering number, not measured in the original; often
/// enough that a free seat is taken soon, rare enough that planning a use costs nothing].
pub const BUSY_RETRY_SECONDS: f32 = 5.0;

/// Why furniture refused an actor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Someone else is on it, or on every entry point that is enabled.
    Busy,
    /// None of its entry points is enabled.
    NoEnabledEntry,
    /// The reference, its base, its model or the model's markers are missing or are not
    /// furniture, or a clip the IDLE tree named does not load.
    MissingContent,
    /// The IDLE tree names no getting-on clip for this actor at this marker (the tree's
    /// conditions may answer otherwise later: blocked for this request, not forever).
    NoClip,
    /// The clip the IDLE tree named cannot play on this actor's skeleton.
    IncompatibleClip,
    /// The actor stands too far from the entry point to take the last steps.
    TooFar,
}

impl Refusal {
    /// Whether waiting can change the answer with nothing else changed.
    pub fn recoverable(self) -> bool {
        self == Refusal::Busy
    }
}

/// What the actor is to do about the furniture `target`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Decision {
    /// Wait; `waited` seconds so far.
    Wait { target: u32, waited: f32 },
    /// No order for this request, however long it stays the same.
    Unavailable { target: u32 },
    /// One fresh order, walking up to it (not put on at once: it never got on).
    Reissue { target: u32 },
    /// It no longer wants this furniture: nothing is left to do about it.
    Cancel,
}

/// What the decision depends on, this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Now {
    /// The furniture the actor's AI has it use now.
    pub wants: Option<u32>,
    /// It could be given an order: drawn, its AI running, nothing holding it, the game
    /// not held.
    pub eligible: bool,
    /// The seconds of this frame.
    pub seconds: f32,
    /// Something a refusal can depend on changed this frame: a script moved the actor, or
    /// hours passed (in the Wait menu, so while it is not eligible).
    pub changed: bool,
}

/// The decision when `refusal` comes back for `target`.
pub fn on_refusal(target: u32, refusal: Refusal) -> Decision {
    if refusal.recoverable() {
        Decision::Wait { target, waited: 0.0 }
    } else {
        Decision::Unavailable { target }
    }
}

/// The decision for an actor waiting for occupied furniture.
pub fn waiting(target: u32, waited: f32, now: &Now) -> Decision {
    if now.wants != Some(target) {
        return Decision::Cancel;
    }
    if !now.eligible {
        return Decision::Wait { target, waited };
    }
    let waited = waited + now.seconds;
    if waited >= BUSY_RETRY_SECONDS {
        Decision::Reissue { target }
    } else {
        Decision::Wait { target, waited }
    }
}

/// The decision for an actor whose furniture refused it for a reason waiting does not
/// change. A reissue need not be eligible: the order goes out when the actor is next drawn.
pub fn unavailable(target: u32, now: &Now) -> Decision {
    if now.wants != Some(target) {
        Decision::Cancel
    } else if now.changed {
        Decision::Reissue { target }
    } else {
        Decision::Unavailable { target }
    }
}

/// Whether a refusal is worth a log line, given the one before it (cleared when a use
/// starts or the actor wants something else): only the first of a run of identical
/// refusals, or a changed one.
pub fn worth_logging(before: Option<(u32, Refusal)>, target: u32, refusal: Refusal) -> bool {
    before != Some((target, refusal))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHAIR: u32 = 0x10;
    const OTHER: u32 = 0x20;
    const FRAME: f32 = 0.25;

    fn now(wants: Option<u32>, eligible: bool) -> Now {
        Now { wants, eligible, seconds: FRAME, changed: false }
    }

    /// One frame of an actor still waiting; it must still be waiting or be given its order.
    fn step(d: Decision, frame: &Now) -> Decision {
        match d {
            Decision::Wait { target, waited } => waiting(target, waited, frame),
            other => panic!("left the wait: {other:?}"),
        }
    }

    #[test]
    fn busy_waits_its_time_and_asks_once() {
        let mut d = on_refusal(CHAIR, Refusal::Busy);
        assert_eq!(d, Decision::Wait { target: CHAIR, waited: 0.0 });
        // 5 s of 0.25 s frames: nothing before the twentieth.
        let frames = (BUSY_RETRY_SECONDS / FRAME) as u32;
        for _ in 1..frames {
            d = step(d, &now(Some(CHAIR), true));
            assert!(matches!(d, Decision::Wait { .. }), "{d:?}");
        }
        assert_eq!(step(d, &now(Some(CHAIR), true)), Decision::Reissue { target: CHAIR });
    }

    #[test]
    fn occupied_for_good_asks_at_a_bounded_rate() {
        // A minute of play with the chair taken all along: a refusal after every order.
        let (mut d, mut orders, mut played) = (on_refusal(CHAIR, Refusal::Busy), 0, 0.0);
        while played < 60.0 {
            played += FRAME;
            d = step(d, &now(Some(CHAIR), true));
            if let Decision::Reissue { target } = d {
                orders += 1;
                d = on_refusal(target, Refusal::Busy);
            }
        }
        assert_eq!(orders, 12);
    }

    #[test]
    fn structural_refusals_are_never_due() {
        for refusal in [Refusal::NoEnabledEntry, Refusal::MissingContent, Refusal::NoClip, Refusal::IncompatibleClip, Refusal::TooFar] {
            let d = on_refusal(CHAIR, refusal);
            assert_eq!(d, Decision::Unavailable { target: CHAIR }, "{refusal:?}");
            for _ in 0..10_000 {
                assert_eq!(unavailable(CHAIR, &now(Some(CHAIR), true)), d, "{refusal:?}");
            }
        }
    }

    #[test]
    fn a_changed_situation_gets_one_fresh_order() {
        let moved = Now { changed: true, ..now(Some(CHAIR), true) };
        assert_eq!(unavailable(CHAIR, &moved), Decision::Reissue { target: CHAIR });
        // Hours pass in a menu, and a script can move it unseen: the change is not lost.
        let held = Now { changed: true, ..now(Some(CHAIR), false) };
        assert_eq!(unavailable(CHAIR, &held), Decision::Reissue { target: CHAIR });
        // It is one order: the next refusal is back to no order for the same request.
        assert_eq!(on_refusal(CHAIR, Refusal::TooFar), Decision::Unavailable { target: CHAIR });
        assert_eq!(unavailable(CHAIR, &now(Some(CHAIR), true)), Decision::Unavailable { target: CHAIR });
    }

    #[test]
    fn wanting_something_else_ends_both_states() {
        for wants in [Some(OTHER), None] {
            assert_eq!(waiting(CHAIR, 1.0, &now(wants, true)), Decision::Cancel);
            assert_eq!(waiting(CHAIR, 1.0, &now(wants, false)), Decision::Cancel, "also when not drawn");
            assert_eq!(unavailable(CHAIR, &now(wants, true)), Decision::Cancel);
            assert_eq!(unavailable(CHAIR, &now(wants, false)), Decision::Cancel, "also when not drawn");
        }
    }

    #[test]
    fn the_clock_stands_while_it_cannot_be_given_an_order() {
        // Not drawn, held or its AI off: nothing is issued however long, and what it had
        // waited is kept.
        for _ in 0..10_000 {
            assert_eq!(waiting(CHAIR, 4.9, &now(Some(CHAIR), false)), Decision::Wait { target: CHAIR, waited: 4.9 });
        }
        assert_eq!(waiting(CHAIR, 4.9, &now(Some(CHAIR), true)), Decision::Reissue { target: CHAIR });
        // A frame of no time (the game's clock stopped) does not bring it nearer either.
        let stopped = Now { seconds: 0.0, ..now(Some(CHAIR), true) };
        assert_eq!(waiting(CHAIR, 4.9, &stopped), Decision::Wait { target: CHAIR, waited: 4.9 });
    }

    #[test]
    fn identical_refusals_are_logged_once() {
        assert!(worth_logging(None, CHAIR, Refusal::Busy), "the first");
        assert!(!worth_logging(Some((CHAIR, Refusal::Busy)), CHAIR, Refusal::Busy), "the same again");
        assert!(worth_logging(Some((CHAIR, Refusal::Busy)), CHAIR, Refusal::TooFar), "another reason");
        assert!(worth_logging(Some((CHAIR, Refusal::Busy)), OTHER, Refusal::Busy), "other furniture");
    }

    #[test]
    fn only_a_busy_refusal_is_retried() {
        assert!(Refusal::Busy.recoverable());
        assert!(!Refusal::IncompatibleClip.recoverable());
    }
}
