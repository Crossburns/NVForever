//! The editor's dialogue debugger: "why won't this NPC say that?" (docs/design/editor.md
//! §14). Read-only: it walks a topic's lines in the order the game tries them and gives
//! each line's verdict from `Game::line_verdict`, the same function the game decides
//! by, and the line or Random pool the game would take now.
//!
//! Evaluating conditions runs their functions, so a condition with a side effect (a
//! GetRandomPercent draw, a stub note) has it here too; no line is said and no random
//! line is picked.

use crate::cond::CondResult;
use crate::dialogue::{group_end, Blocked, Says};
use crate::game::Game;
use fnv_sim::FormId;
use serde::Serialize;

/// One line of a topic as the debugger sees it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LineExplained {
    pub info: FormId,
    pub quest: Option<FormId>,
    /// The quest's priority (0 without a quest).
    pub priority: u8,
    /// The group the line is tried in: a plain line alone, a Random pool together.
    pub group: usize,
    pub random: bool,
    /// The line's own conditions when it can be said, or what holds it back.
    pub verdict: Result<Vec<CondResult>, Blocked>,
    /// Whether the line is written for this speaker at all ([`Game::line_fits`]): the
    /// console hides the others unless asked for every line.
    pub fits: bool,
}

/// A topic explained for one speaker and listener ([`Game::explain_topic`]).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TopicExplained {
    pub topic: FormId,
    pub speaker: FormId,
    pub listener: Option<FormId>,
    /// Every line, in the order the game tries them.
    pub lines: Vec<LineExplained>,
    /// The lines the game takes from now: one, or the sayable lines of a Random pool (it
    /// picks one of them at random); empty when the speaker has nothing to say.
    pub winners: Vec<FormId>,
}

impl<'a> Game<'a> {
    /// Every line of `topic` for `speaker` talking to `listener`, as greetings, SayTo
    /// and Say choose them ([`Game::select_line`]): unlike the game, every line is
    /// judged, not just those up to the one taken. A topic the player picks from the
    /// list also passes over follow-up lines while another can be said
    /// (`select_choice_line`); that rule is not shown here.
    pub fn explain_topic(&mut self, speaker: FormId, listener: Option<FormId>, topic: FormId) -> TopicExplained {
        let data = self.data;
        let tried = data.lines_tried(topic);
        let mut lines = Vec::with_capacity(tried.len());
        let mut winners = Vec::new();
        let (mut at, mut group) = (0, 0);
        while at < tried.len() {
            let end = group_end(data, &tried, at);
            let mut sayable = Vec::new();
            for &info in &tried[at..end] {
                let def = data.infos.get(&info);
                let verdict = self.line_verdict(speaker, listener, info, Says::Actor);
                if verdict.is_ok() {
                    sayable.push(info);
                }
                let fits = verdict.is_ok() || self.line_fits(speaker, listener, info, &verdict);
                lines.push(LineExplained {
                    info,
                    quest: def.and_then(|d| d.quest),
                    priority: data.line_priority(info),
                    group,
                    random: def.is_some_and(|d| d.has(crate::data::info_flag::RANDOM)),
                    verdict,
                    fits,
                });
            }
            if winners.is_empty() && !sayable.is_empty() {
                winners = sayable;
            }
            at = end;
            group += 1;
        }
        TopicExplained { topic, speaker, listener, lines, winners }
    }

    /// Whether a line that cannot be said is still written for `speaker`: its fixed
    /// speaker, its voice-type gate and its "who says it" conditions (GetIsID,
    /// GetInFaction, GetIsVoiceType … on the subject, `VOICE_GATE_FUNCTIONS`), the line's
    /// and its quest's, allow this speaker. A group of OR-joined conditions is judged
    /// only when every condition in it is such a condition. For the debugger's display
    /// only; the game decides by [`Game::line_verdict`].
    fn line_fits(&mut self, speaker: FormId, listener: Option<FormId>, info: FormId, verdict: &Result<Vec<CondResult>, Blocked>) -> bool {
        let data = self.data;
        if matches!(verdict, Err(Blocked::Speaker(_) | Blocked::VoiceType | Blocked::NotALine)) {
            return false;
        }
        let Some(def) = data.infos.get(&info) else { return false };
        let quest = def.quest.and_then(|q| data.quests.get(&q)).map(|q| q.conditions.as_slice()).unwrap_or_default();
        for conds in [quest, def.conditions.as_slice()] {
            let mut at = 0;
            while at < conds.len() {
                let mut end = at;
                while end < conds.len() && conds[end].or {
                    end += 1;
                }
                let group = &conds[at..(end + 1).min(conds.len())];
                let who = |c: &crate::cond::Condition| crate::dialogue::VOICE_GATE_FUNCTIONS.contains(&c.function) && matches!(c.run_on, crate::cond::RunOn::Subject);
                if group.iter().all(who) && !group.iter().any(|c| self.eval_conditions(std::slice::from_ref(c), Some(speaker), listener, "debugger").first().is_some_and(|r| r.passed)) {
                    return false;
                }
                at = end + 1;
            }
        }
        true
    }

    /// [`Game::explain_topic`] as console lines: the lines written for the speaker, or
    /// with `every` all of them.
    pub fn explain_topic_text(&mut self, speaker: FormId, listener: Option<FormId>, topic: FormId, every: bool) -> Vec<String> {
        let e = self.explain_topic(speaker, listener, topic);
        let hidden = if every { 0 } else { e.lines.iter().filter(|l| !l.fits).count() };
        let data = self.data;
        let named = |f: FormId| match data.edid_of.get(&f) {
            Some(n) => format!("{f} ({n})"),
            None => f.to_string(),
        };
        let mut out = vec![format!(
            "Topic {} for {} talking to {}: {} line(s), tried top to bottom",
            named(topic),
            named(speaker),
            listener.map_or("nobody".into(), named),
            e.lines.len()
        )];
        if hidden > 0 {
            out.push(format!("  {hidden} line(s) written for other speakers are not shown (add `all` to list every line)"));
        }
        for (n, l) in e.lines.iter().enumerate().filter(|(_, l)| every || l.fits) {
            let def = data.infos.get(&l.info);
            let text = def.and_then(|d| d.responses.first()).map(|r| short(&r.text)).unwrap_or_default();
            let quest = l.quest.map(|q| format!(" quest {} (priority {})", named(q), l.priority)).unwrap_or_default();
            let pool = if l.random { format!(" random pool {}", l.group) } else { String::new() };
            let mark = if e.winners.contains(&l.info) {
                "SAID"
            } else if l.verdict.is_ok() {
                "could"
            } else {
                "no"
            };
            out.push(format!("{:>3}. [{mark}] {}{quest}{pool} \"{text}\"", n + 1, l.info));
            let conds = def.map(|d| d.conditions.as_slice()).unwrap_or_default();
            match &l.verdict {
                Ok(r) => self.push_conditions(&mut out, conds, r),
                Err(Blocked::Conditions(r)) => {
                    out.push("       its conditions fail:".into());
                    self.push_conditions(&mut out, conds, r);
                }
                Err(Blocked::QuestConditions(q, r)) => {
                    out.push(format!("       the conditions of its quest {} fail:", named(*q)));
                    let qc = data.quests.get(q).map(|d| d.conditions.as_slice()).unwrap_or_default();
                    self.push_conditions(&mut out, qc, r);
                }
                Err(b) => out.push(format!("       {}", blocked_text(b, &named))),
            }
        }
        out.push(match &e.winners[..] {
            [] => "Nothing to say: no line can be said now".into(),
            [one] => format!("Said now: {one}"),
            pool => format!("Said now: one of {} at random", pool.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(", ")),
        });
        out
    }

    fn push_conditions(&self, out: &mut Vec<String>, conds: &[crate::cond::Condition], results: &[CondResult]) {
        for (i, (c, r)) in conds.iter().zip(results).enumerate() {
            let on = r.on.map_or("nothing".into(), |f| FormId(f).to_string());
            out.push(format!("       {} {} {} on {on}", i + 1, if r.passed { "pass" } else { "FAIL" }, self.condition_text(c, r)));
        }
    }
}

fn blocked_text(b: &Blocked, named: &dyn Fn(FormId) -> String) -> String {
    match b {
        Blocked::NotALine => "not a dialogue line".into(),
        Blocked::SaidOnce => "Say Once, and it has been said".into(),
        Blocked::Intelligence { low: true } => "a Low Intelligence line, and the player is not dim".into(),
        Blocked::Intelligence { low: false } => "a High Intelligence line, and the player is dim".into(),
        Blocked::SaidToday { hours_ago } => format!("Say Once a Day, said {hours_ago:.1} game hours ago"),
        Blocked::Speaker(s) => format!("only {} says it", named(*s)),
        Blocked::QuestNotRunning(q) => format!("its quest {} is not running", named(*q)),
        Blocked::VoiceType => "the speaker's voice type has no Allow Default Dialog, and the line has no condition on who says it".into(),
        Blocked::QuestConditions(q, _) => format!("the conditions of its quest {} fail", named(*q)),
        Blocked::Conditions(_) => "its conditions fail".into(),
    }
}

/// The start of a response's text, for one line of the console.
fn short(t: &str) -> String {
    const MAX: usize = 60;
    match t.char_indices().nth(MAX) {
        Some((i, _)) => format!("{}...", &t[..i]),
        None => t.to_string(),
    }
}
