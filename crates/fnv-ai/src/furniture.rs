//! Which furniture an actor may be given, decided before a choice or a reservation
//! (docs/spec-notes/furniture.md §9) [design]:
//!
//! - **Compatibility**, for casual and for assigned furniture: a creature the assets give
//!   no clip for a piece (`Fit::Unsupported`) does not get it. `Fit::Unknown` stays
//!   permissive, and a person is never `Unsupported`.
//! - **The crucifix is not casual furniture**: a piece with an enabled marker of ID 10 is
//!   left out of sandbox choices and of a Sleep or Eat package's search. It stays open to
//!   what a package names or links to, so the crucified stay on their crosses, and to a
//!   patrol point, a Find and a Use Item At, whose searches are for what their package
//!   asks. An enabled marker 10 anywhere on a model counts; the one model with it has one
//!   marker (furniture.md §2).
//!
//! The facts come from the caller ([`Ai::set_furniture_facts`](crate::Ai::set_furniture_facts));
//! without them, every answer is what it was before this rule. The filter draws no random
//! numbers: it only shortens the lists a choice is made from.

use fnv_game::game::Game;
use fnv_sim::furniture_facts::{ActorKind, Fit, FurnitureFacts};
use fnv_sim::FormId;
use std::collections::BTreeSet;
use std::sync::Arc;

/// The marker ID of the crucifix (furniture.md §2).
pub const CRUCIFIX_MARKER: u8 = 10;

/// What the furniture is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    /// Chosen by the actor: a sandbox's seat or bed, a Sleep or Eat package's search.
    Casual,
    /// Named by a package, a link or a patrol point; looked for by a Find or a Use Item At.
    Assigned,
}

/// The answer for one actor and one piece.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allowed,
    /// Allowed because nothing is known either way.
    Unknown,
    Excluded(&'static str),
}

/// The reasons a line of the log gives.
pub const REASON_CASUAL_MARKER: &str = "casual-marker-10";
pub const REASON_NO_CLIPS: &str = "no-clips-for-skeleton";
pub const REASON_NO_EVIDENCE: &str = "no-evidence";

/// The rule itself: both parts, in the order the log's reason gives them.
pub fn decide(facts: &dyn FurnitureFacts, kind: ActorKind, skeleton: &str, base: FormId, context: Context) -> Decision {
    if context == Context::Casual && facts.marker_ids(base).is_some_and(|m| m.contains(&CRUCIFIX_MARKER)) {
        return Decision::Excluded(REASON_CASUAL_MARKER);
    }
    match facts.fit(kind, skeleton, base) {
        Fit::Supported => Decision::Allowed,
        Fit::Unknown => Decision::Unknown,
        Fit::Unsupported => Decision::Excluded(REASON_NO_CLIPS),
    }
}

/// A decision to report: once per actor and piece when something was excluded, once per
/// actor when nothing is known ([`crate::AiEvent::FurnitureEligibility`]).
pub(crate) struct Note {
    pub(crate) actor: FormId,
    pub(crate) furniture: FormId,
    pub(crate) decision: &'static str,
    pub(crate) reason: &'static str,
}

/// The facts, and what has been reported of them.
#[derive(Default)]
pub struct Eligibility {
    facts: Option<Arc<dyn FurnitureFacts>>,
    reported: BTreeSet<(FormId, FormId)>,
    pub(crate) notes: Vec<Note>,
}

impl Eligibility {
    pub(crate) fn set_facts(&mut self, facts: Arc<dyn FurnitureFacts>) {
        self.facts = Some(facts);
    }

    fn note(&mut self, actor: FormId, furniture: FormId, decision: Decision) {
        let (key, decision, reason) = match decision {
            Decision::Allowed => return,
            Decision::Unknown => (FormId(0), "unknown", REASON_NO_EVIDENCE),
            Decision::Excluded(why) => (furniture, "excluded", why),
        };
        if self.reported.insert((actor, key)) {
            self.notes.push(Note { actor, furniture, decision, reason });
        }
    }

    /// Of `references`, the furniture `actor` may not choose for itself. Empty without
    /// facts, and when every piece is allowed.
    pub(crate) fn excluded_casual(&mut self, game: &Game, actor: FormId, references: impl Iterator<Item = FormId>) -> BTreeSet<FormId> {
        let mut out = BTreeSet::new();
        let Some(facts) = self.facts.clone() else { return out };
        // Read once a piece is around: an actor's record is decoded on first asking.
        let mut who = None;
        for r in references {
            let Some(base) = game.world.placed.get(r).map(|p| p.base).filter(|b| game.data.furniture.contains_key(b)) else { continue };
            let (kind, skeleton) = who.get_or_insert_with(|| Self::who(game, actor));
            let d = decide(&*facts, *kind, skeleton, base, Context::Casual);
            self.note(actor, r, d);
            if matches!(d, Decision::Excluded(_)) {
                out.insert(r);
            }
        }
        out
    }

    /// What the actor is and the skeleton it has.
    fn who(game: &Game, actor: FormId) -> (ActorKind, String) {
        let kind = match fnv_game::nvse_nx::is_creature(game, actor) {
            Some(true) => ActorKind::Creature,
            Some(false) => ActorKind::Person,
            None => ActorKind::Unresolved,
        };
        (kind, fnv_game::nvse_nx::skeleton_model(game, actor))
    }

    /// Whether `actor` may be given `reference`, which its package names or looks for:
    /// yes for anything that is not furniture.
    pub(crate) fn fits_assigned(&mut self, game: &Game, actor: FormId, reference: FormId) -> bool {
        let Some(facts) = self.facts.clone() else { return true };
        let Some(base) = fnv_game::functions::base_of(game, reference).filter(|b| game.data.furniture.contains_key(b)) else { return true };
        let (kind, skeleton) = Self::who(game, actor);
        let d = decide(&*facts, kind, &skeleton, base, Context::Assigned);
        self.note(actor, reference, d);
        !matches!(d, Decision::Excluded(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Table(Fit, Option<Vec<u8>>);

    impl FurnitureFacts for Table {
        fn marker_ids(&self, _: FormId) -> Option<&[u8]> {
            self.1.as_deref()
        }
        fn fit(&self, _: ActorKind, _: &str, _: FormId) -> Fit {
            self.0
        }
    }

    const BASE: FormId = FormId(0x100);

    #[test]
    fn the_two_rules_answer_apart() {
        let seat = Table(Fit::Supported, Some(vec![11, 12]));
        assert_eq!(decide(&seat, ActorKind::Creature, "s", BASE, Context::Casual), Decision::Allowed);
        assert_eq!(decide(&seat, ActorKind::Creature, "s", BASE, Context::Assigned), Decision::Allowed);
        // The cross: not chosen, but kept when assigned.
        let cross = Table(Fit::Supported, Some(vec![CRUCIFIX_MARKER]));
        assert_eq!(decide(&cross, ActorKind::Creature, "s", BASE, Context::Casual), Decision::Excluded(REASON_CASUAL_MARKER));
        assert_eq!(decide(&cross, ActorKind::Creature, "s", BASE, Context::Assigned), Decision::Allowed);
        // No clips: out either way.
        let none = Table(Fit::Unsupported, Some(vec![11]));
        assert_eq!(decide(&none, ActorKind::Creature, "s", BASE, Context::Casual), Decision::Excluded(REASON_NO_CLIPS));
        assert_eq!(decide(&none, ActorKind::Creature, "s", BASE, Context::Assigned), Decision::Excluded(REASON_NO_CLIPS));
    }

    #[test]
    fn what_is_not_known_stays_allowed() {
        let unread = Table(Fit::Unknown, None);
        assert_eq!(decide(&unread, ActorKind::Unresolved, "", BASE, Context::Casual), Decision::Unknown);
        assert_eq!(decide(&unread, ActorKind::Unresolved, "", BASE, Context::Assigned), Decision::Unknown);
    }

    #[test]
    fn each_piece_is_reported_once_and_unknown_once_per_actor() {
        let mut e = Eligibility::default();
        let (a, b, f, g) = (FormId(1), FormId(2), FormId(3), FormId(4));
        for _ in 0..3 {
            e.note(a, f, Decision::Excluded(REASON_NO_CLIPS));
            e.note(a, f, Decision::Allowed);
            e.note(a, g, Decision::Unknown);
            e.note(a, f, Decision::Unknown);
        }
        e.note(b, f, Decision::Excluded(REASON_NO_CLIPS));
        let lines: Vec<_> = e.notes.iter().map(|n| (n.actor, n.furniture, n.decision)).collect();
        assert_eq!(lines, [(a, f, "excluded"), (a, g, "unknown"), (b, f, "excluded")]);
    }
}
