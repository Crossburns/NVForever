//! The Vigor Tester's pages, what each control does, and which parts of the machine show
//! what (docs/spec-notes/vigor-tester.md). Plain functions over the S.P.E.C.I.A.L. sheet
//! of `fnv_game::chargen`, which holds the rules (points, limits, completeness); the
//! drawing (`super`) only asks this module.

use fnv_game::chargen::{SpecialSheet, SPECIAL_MAX};

/// The attributes in sheet order (0 Strength … 6 Luck), as the model's node names spell
/// them (`PB-Strength_01`, `Index_StrengthPointVal`, `SPECIAL-Title_Strength`).
pub const STATS: [&str; 7] = ["Strength", "Perception", "Endurance", "Charisma", "Intelligence", "Agility", "Luck"];

/// The letter of each attribute's ten label boards (`S_01` … `L_10`) [observed: the
/// activate model's node names; docs/spec-notes/vigor-tester.md §3].
pub const BOARD_LETTERS: [char; 7] = ['S', 'P', 'E', 'C', 'I', 'A', 'L'];

/// A page of the machine: the front poster and label boards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Page {
    /// "Rate Your… TRY IT NOW!", blank boards (the model's stored pose).
    Attract,
    /// One attribute's page: its poster, its ten labels, its bulbs lit.
    Stat(usize),
    /// "Let's Review…": the seven values with arrows, the points remaining.
    Review,
}

impl Page {
    /// The order the → button goes through them [observed: the sequences' chain,
    /// docs/spec-notes/vigor-tester.md §4].
    pub const ALL: [Page; 9] = [
        Page::Attract,
        Page::Stat(0),
        Page::Stat(1),
        Page::Stat(2),
        Page::Stat(3),
        Page::Stat(4),
        Page::Stat(5),
        Page::Stat(6),
        Page::Review,
    ];

    pub fn index(self) -> usize {
        match self {
            Page::Attract => 0,
            Page::Stat(i) => 1 + i.min(6),
            Page::Review => 8,
        }
    }

    pub fn from_index(i: usize) -> Option<Page> {
        Page::ALL.get(i).copied()
    }

    pub fn next(self) -> Option<Page> {
        Page::from_index(self.index() + 1)
    }

    pub fn prev(self) -> Option<Page> {
        self.index().checked_sub(1).and_then(Page::from_index)
    }

    pub fn name(self) -> &'static str {
        match self {
            Page::Attract => "Attract",
            Page::Stat(i) => STATS[i.min(6)],
            Page::Review => "Review",
        }
    }

    /// A page by name (any case: `review`, `attract`, an attribute) or number (0 Attract,
    /// 1–7 the attributes, 8 Review).
    pub fn parse(s: &str) -> Option<Page> {
        let s = s.trim();
        if let Ok(n) = s.parse::<usize>() {
            return Page::from_index(n);
        }
        Page::ALL.into_iter().find(|p| p.name().eq_ignore_ascii_case(s))
    }
}

/// The embedded sequence that turns the boards from page `from` to the next or previous
/// page `to` [observed: in each, the layer of `from` leaves the front (y −10.87) and the
/// layer of `to` comes to it; docs/spec-notes/vigor-tester.md §4]. The names are the
/// generic ones the model's author gave them.
pub fn sequence(from: Page, to: Page) -> Option<&'static str> {
    const FORWARD: [&str; 8] = ["Forward", "Backward", "Left", "Right", "FastForward", "FastBackward", "FastLeft", "FastRight"];
    const BACK: [&str; 8] = ["AimUp", "Aim", "Holster", "JumpLand", "JumpLoop", "JumpStart", "TurnRight", "TurnLeft"];
    let (a, b) = (from.index(), to.index());
    if b == a + 1 {
        FORWARD.get(a).copied()
    } else if a == b + 1 {
        BACK.get(b).copied()
    } else {
        None
    }
}

/// The sequence whose end pose is the Attract page (the menu opens there).
pub const ATTRACT_POSE: &str = "AimUp";

/// The bulbs lit on a page: the attribute's value on its page, none elsewhere
/// [observed: 7.jpg–9.jpg five lit at 5; 6.jpg, 10.jpg, 11.jpg none on the review page].
pub fn lit_bulbs(page: Page, sheet: &SpecialSheet) -> usize {
    match page {
        Page::Stat(i) => sheet.values[i.min(6)].clamp(0, SPECIAL_MAX) as usize,
        _ => 0,
    }
}

/// The two shapes of bulb `n` (1–10) shown when it is lit: a glow sprite and the bulb
/// drawn without lighting [observed: `P1_PointVal_<nn>_GLOW:4` with
/// `fxparttinyglowy.dds`, `:5` with the bulb texture and white emissive light, in
/// `nv_vitomaticvigortester_cabinet.nif`].
pub fn glow_shapes(n: usize) -> [String; 2] {
    [format!("P1_PointVal_{n:02}_GLOW:4"), format!("P1_PointVal_{n:02}_GLOW:5")]
}

/// The number texture for `n` (0–10) [observed: `textures\terminals\BBNumber0.dds` …
/// `BBNumber10.dds`; the model's number shapes carry `BBNumber0`].
pub fn number_texture(n: i32) -> String {
    format!("textures\\terminals\\BBNumber{}.dds", n.clamp(0, 10))
}

/// The two digits of the points remaining (tens, ones), held to 0–99.
pub fn two_digits(n: i32) -> (i32, i32) {
    let n = n.clamp(0, 99);
    (n / 10, n % 10)
}

/// The digit shapes of the points remaining: `P1_PointsRemainDigit1` is the tens and
/// `Digit2` the ones [observed: Digit2 lies right of Digit1 on both pages, x −21.7…−18.2
/// against −18.2…−14.8 on an attribute's page; the screen's right is the model's −x].
pub const TENS_SHAPE: &str = "P1_PointsRemainDigit1:0";
pub const ONES_SHAPE: &str = "P1_PointsRemainDigit2:0";

/// The review page's arrows of attribute `i`: (◄ lower shown, ► raise shown). An arrow
/// that would do nothing is hidden [observed: 6.jpg at 00 points remaining has no ►
/// arrows; 10.jpg and 11.jpg show both on every row; the ◄ at 1 is a hypothesis by
/// symmetry].
pub fn review_arrows(sheet: &SpecialSheet, i: usize) -> (bool, bool) {
    let i = i.min(6);
    let v = sheet.values[i];
    (sheet.can_lower(i), v < SPECIAL_MAX && sheet.remaining() > 0)
}

/// Whether the → button works on `page`: always, but on the review page only once every
/// point is given out (there it is Done) [observed: dimmed in 10.jpg and 11.jpg, lit in
/// 6.jpg and on the attributes' pages 7.jpg–9.jpg at 05 points to go].
pub fn next_enabled(page: Page, sheet: &SpecialSheet) -> bool {
    match page {
        Page::Review => sheet.complete(),
        _ => true,
    }
}

/// Whether the ← button works: not on the first page (its button sits lower there
/// [observed: `P1_LT_Btn` at z 56.83 in the Attract pose, 58.72 elsewhere]).
pub fn prev_enabled(page: Page) -> bool {
    page != Page::Attract
}

/// The → button's look [hypothesis: the PC's `terminals\pc\` pictures: `BBRTOff` black
/// with a light arrow as on the attributes' pages, `BBRTOn` grey as the dimmed arrow of
/// 10.jpg; the model names the console pictures `terminals\BBRTOff.dds`].
pub fn rt_texture(enabled: bool) -> &'static str {
    if enabled {
        "textures\\terminals\\pc\\BBRTOff.dds"
    } else {
        "textures\\terminals\\pc\\BBRTOn.dds"
    }
}

pub const LT_TEXTURE: &str = "textures\\terminals\\pc\\BBLTOff.dds";

/// A button's picture while pressed (or a review row chosen with the keys) and not
/// [hypothesis: the `On` pictures are the pressed look].
pub fn press_texture(raise: bool, on: bool) -> &'static str {
    match (raise, on) {
        (true, false) => "textures\\terminals\\BBIncreaseOff.dds",
        (true, true) => "textures\\terminals\\BBIncreaseOn.dds",
        (false, false) => "textures\\terminals\\BBDecreaseOff.dds",
        (false, true) => "textures\\terminals\\BBDecreaseOn.dds",
    }
}

/// The boards, posters and buttons under the rig's light (nvforever `vigor_view.rs`): as
/// they are [design: the rig is set so that they come to about 0.85 of the texture's
/// brightness, as in 10.jpg; to tune against the original's shots].
pub const BOARD_TINT: [f32; 3] = [1.0, 1.0, 1.0];
/// The cabinet and the rest of its model: darker than the boards under the same light
/// [design: in 10.jpg the cabinet is about 0.45 of the texture's brightness and the boards
/// 0.85; to tune against the original's shots].
pub const CABINET_TINT: [f32; 3] = [0.6, 0.6, 0.6];
/// A dull bulb: warm gold under the rig's light [design: to tune against the original's
/// shots].
pub const BULB_DULL_TINT: [f32; 3] = [1.0, 0.92, 0.72];
/// A lit bulb's unlit shape (`_GLOW:5`): over 1, so the pale texture goes to white and its
/// dark numbers stay readable [design: to tune against the original's shots].
pub const BULB_LIT_TINT: [f32; 3] = [1.5, 1.6, 2.0];
/// A lit bulb's additive sprite (`_GLOW:4`), the soft halo over the board [design: to tune
/// against the original's shots; stronger hides the numbers].
pub const BULB_HALO_TINT: [f32; 3] = [0.8, 0.75, 0.6];

/// A glow shape of a lit bulb ([`glow_shapes`]).
pub fn is_glow(shape: &str) -> bool {
    shape.contains("_GLOW")
}

/// The colour factor (display values, red green blue) on a shape's material. `cabinet`: a
/// shape of the cabinet model.
pub fn shade(shape: &str, cabinet: bool) -> [f32; 3] {
    if is_glow(shape) {
        if shape.ends_with(":4") {
            BULB_HALO_TINT
        } else {
            BULB_LIT_TINT
        }
    } else if !cabinet {
        BOARD_TINT
    } else if shape.starts_with("P1_PointVal_") && shape.ends_with(":3") {
        BULB_DULL_TINT
    } else {
        CABINET_TINT
    }
}

/// What the player asks of the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// One point more for an attribute (`None`: the page's or the chosen row's).
    Raise(Option<usize>),
    Lower(Option<usize>),
    /// Choose a row of the review page (Up/Down: `index_up`, `index_down`).
    Select(usize),
    Step(i32),
    Next,
    Prev,
    Done,
}

/// What came of an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Changed { stat: usize, value: i32, remaining: i32 },
    Turned { from: Page, to: Page },
    Selected(usize),
    Done,
    /// Nothing happened (a limit, a dimmed button, no attribute to change).
    Refused(&'static str),
}

/// The machine's state: the sheet, the page in front, the review page's chosen row.
#[derive(Debug, Clone, PartialEq)]
pub struct Vigor {
    pub sheet: SpecialSheet,
    pub page: Page,
    pub row: usize,
}

impl Vigor {
    pub fn new(sheet: SpecialSheet) -> Vigor {
        Vigor { sheet, page: Page::Attract, row: 0 }
    }

    /// The attribute the lever and the Left/Right keys change on this page.
    pub fn target(&self, stat: Option<usize>) -> Option<usize> {
        match (stat, self.page) {
            (Some(i), Page::Review) => Some(i.min(6)),
            (Some(i), Page::Stat(p)) if i == p => Some(i),
            (Some(_), _) => None,
            (None, Page::Stat(i)) => Some(i),
            (None, Page::Review) => Some(self.row),
            (None, Page::Attract) => None,
        }
    }

    pub fn act(&mut self, a: Action) -> Outcome {
        match a {
            Action::Raise(s) | Action::Lower(s) => {
                let Some(i) = self.target(s) else { return Outcome::Refused("no attribute on this page") };
                let raise = matches!(a, Action::Raise(_));
                let ok = if raise { self.sheet.raise(i) } else { self.sheet.lower(i) };
                if self.page == Page::Review {
                    self.row = i;
                }
                if ok {
                    Outcome::Changed { stat: i, value: self.sheet.values[i], remaining: self.sheet.remaining() }
                } else if raise {
                    Outcome::Refused(if self.sheet.values[i] >= SPECIAL_MAX { "at 10" } else { "no points left" })
                } else {
                    Outcome::Refused("at 1")
                }
            }
            Action::Select(i) => {
                if self.page != Page::Review {
                    return Outcome::Refused("rows are chosen on the review page");
                }
                self.row = i.min(6);
                Outcome::Selected(self.row)
            }
            Action::Step(d) => {
                if self.page != Page::Review {
                    return Outcome::Refused("rows are chosen on the review page");
                }
                self.row = (self.row as i32 + d).clamp(0, 6) as usize;
                Outcome::Selected(self.row)
            }
            Action::Next => match self.page.next() {
                Some(to) => self.turn(to),
                None => self.act(Action::Done),
            },
            Action::Prev => match self.page.prev() {
                Some(to) => self.turn(to),
                None => Outcome::Refused("first page"),
            },
            Action::Done => {
                if self.sheet.complete() {
                    Outcome::Done
                } else {
                    Outcome::Refused("points still to give out")
                }
            }
        }
    }

    fn turn(&mut self, to: Page) -> Outcome {
        let from = self.page;
        self.page = to;
        Outcome::Turned { from, to }
    }

    /// The sheet as the log writes it: "5 5 5 5 5 5 5".
    pub fn values(&self) -> String {
        self.sheet.values.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" ")
    }
}

/// A clickable part of the machine and what a click on it does on `page`.
pub fn button_action(shape: &str, page: Page) -> Option<Action> {
    let base = shape.split(':').next().unwrap_or(shape);
    match base {
        "P1_LT_Btn" => Some(Action::Prev),
        "P1_RT_Btn" | "LookInside_Btn" => Some(Action::Next),
        "P1_Increase_Btn" => Some(Action::Raise(None)),
        "P1_Decrease_Btn" => Some(Action::Lower(None)),
        _ => {
            if page != Page::Review {
                return None;
            }
            let rest = base.strip_prefix("Index_")?;
            let i = STATS.iter().position(|s| rest.starts_with(s))?;
            match &rest[STATS[i].len()..] {
                "Increase_Btn" => Some(Action::Raise(Some(i))),
                "Decrease_Btn" => Some(Action::Lower(Some(i))),
                "PointVal" => Some(Action::Select(i)),
                _ => None,
            }
        }
    }
}

/// The shapes a click can land on, on `page` (the buttons in front there).
pub fn live_buttons(page: Page) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    match page {
        Page::Attract => v.push("LookInside_Btn:0".into()),
        Page::Stat(_) => v.extend(["P1_LT_Btn:0", "P1_RT_Btn:0", "P1_Increase_Btn:0", "P1_Decrease_Btn:0"].map(String::from)),
        Page::Review => {
            v.extend(["P1_LT_Btn:0", "P1_RT_Btn:0", "P1_Increase_Btn:0", "P1_Decrease_Btn:0"].map(String::from));
            for s in STATS {
                v.push(format!("Index_{s}Increase_Btn:0"));
                v.push(format!("Index_{s}Decrease_Btn:0"));
                v.push(format!("Index_{s}PointVal:0"));
            }
        }
    }
    v
}

/// `NVF_PLAY_TEST_VIGOR`: one timed step.
#[derive(Debug, Clone, PartialEq)]
pub enum AidOp {
    Act(Action),
    /// Go straight to a page, turning through the pages between.
    Page(Page),
    /// A click at the middle of a shape's outline on the screen.
    Click(String),
}

/// An attribute by number (0–6) or name (any case, or its first letter).
pub fn parse_stat(s: &str) -> Option<usize> {
    let s = s.trim();
    if let Ok(n) = s.parse::<usize>() {
        return (n < 7).then_some(n);
    }
    STATS.iter().position(|n| n.eq_ignore_ascii_case(s)).or_else(|| {
        let mut c = s.chars();
        match (c.next(), c.next()) {
            (Some(l), None) => BOARD_LETTERS.iter().position(|b| b.eq_ignore_ascii_case(&l)),
            _ => None,
        }
    })
}

/// `<seconds>:<op>[,...]` with ops `raise[:<stat>]`, `lower[:<stat>]`, `select:<stat>`,
/// `page:next|prev|<page>`, `done`, `click:<shape>`.
pub fn parse_aid(v: &str) -> Result<Vec<(f32, AidOp)>, String> {
    crate::test_aid::timed(v)?
        .into_iter()
        .map(|(t, what)| {
            let (op, arg) = match what.split_once(':') {
                Some((o, a)) => (o.trim(), Some(a.trim())),
                None => (what.trim(), None),
            };
            let stat = |a: Option<&str>| match a {
                None => Ok(None),
                Some(a) => parse_stat(a).map(Some).ok_or_else(|| format!("{what:?}: unknown attribute {a:?}")),
            };
            let step = match op {
                "raise" => AidOp::Act(Action::Raise(stat(arg)?)),
                "lower" => AidOp::Act(Action::Lower(stat(arg)?)),
                "select" => AidOp::Act(Action::Select(stat(arg)?.ok_or_else(|| format!("{what:?}: select needs an attribute"))?)),
                "done" => AidOp::Act(Action::Done),
                "page" => match arg {
                    Some("next") => AidOp::Act(Action::Next),
                    Some("prev") => AidOp::Act(Action::Prev),
                    Some(p) => AidOp::Page(Page::parse(p).ok_or_else(|| format!("{what:?}: unknown page {p:?}"))?),
                    None => return Err(format!("{what:?}: page needs next, prev or a page")),
                },
                "click" => AidOp::Click(arg.filter(|a| !a.is_empty()).ok_or_else(|| format!("{what:?}: click needs a shape"))?.to_string()),
                _ => return Err(format!("{what:?}: expected raise, lower, select, page, done or click")),
            };
            Ok((t, step))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fives() -> SpecialSheet {
        SpecialSheet::new([5; 7])
    }

    #[test]
    fn the_pages_run_attract_to_review() {
        assert_eq!(Page::Attract.next(), Some(Page::Stat(0)));
        assert_eq!(Page::Stat(6).next(), Some(Page::Review));
        assert_eq!(Page::Review.next(), None);
        assert_eq!(Page::Attract.prev(), None);
        assert_eq!(Page::Review.prev(), Some(Page::Stat(6)));
        for p in Page::ALL {
            assert_eq!(Page::from_index(p.index()), Some(p));
        }
        assert_eq!(Page::parse("review"), Some(Page::Review));
        assert_eq!(Page::parse("AGILITY"), Some(Page::Stat(5)));
        assert_eq!(Page::parse("0"), Some(Page::Attract));
        assert_eq!(Page::parse("9"), None);
    }

    #[test]
    fn each_turn_has_its_sequence() {
        assert_eq!(sequence(Page::Attract, Page::Stat(0)), Some("Forward"));
        assert_eq!(sequence(Page::Stat(0), Page::Stat(1)), Some("Backward"));
        assert_eq!(sequence(Page::Stat(6), Page::Review), Some("FastRight"));
        assert_eq!(sequence(Page::Review, Page::Stat(6)), Some("TurnLeft"));
        assert_eq!(sequence(Page::Stat(1), Page::Stat(0)), Some("Aim"));
        assert_eq!(sequence(Page::Stat(0), Page::Attract), Some("AimUp"));
        assert_eq!(sequence(Page::Attract, Page::Review), None);
        // Sixteen turns, sixteen different sequences.
        let mut all: Vec<&str> = Page::ALL.windows(2).flat_map(|w| [sequence(w[0], w[1]).unwrap(), sequence(w[1], w[0]).unwrap()]).collect();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), 16);
    }

    #[test]
    fn the_parts_show_the_sheet() {
        let mut s = fives();
        s.values[0] = 8;
        assert_eq!(lit_bulbs(Page::Stat(0), &s), 8);
        assert_eq!(lit_bulbs(Page::Stat(1), &s), 5);
        assert_eq!(lit_bulbs(Page::Review, &s), 0);
        assert_eq!(lit_bulbs(Page::Attract, &s), 0);
        assert_eq!(glow_shapes(3), ["P1_PointVal_03_GLOW:4".to_string(), "P1_PointVal_03_GLOW:5".to_string()]);
        assert_eq!(number_texture(7), "textures\\terminals\\BBNumber7.dds");
        assert_eq!(number_texture(12), "textures\\terminals\\BBNumber10.dds");
        assert_eq!(two_digits(5), (0, 5));
        assert_eq!(two_digits(-3), (0, 0));
        assert_eq!(two_digits(40), (4, 0));
        // 8 + 6×5 = 38: 2 left, both arrows; at 10 or with none left no ►, at 1 no ◄.
        assert_eq!(review_arrows(&s, 0), (true, true));
        s.values[1] = 7;
        assert_eq!(s.remaining(), 0);
        assert_eq!(review_arrows(&s, 2), (true, false));
        s.values[1] = 1;
        s.values[0] = 10;
        assert_eq!(review_arrows(&s, 1), (false, true));
        assert_eq!(review_arrows(&s, 0), (true, false));
        assert!(!next_enabled(Page::Review, &fives()));
        assert!(next_enabled(Page::Stat(3), &fives()));
        assert!(!prev_enabled(Page::Attract));
        assert_eq!(rt_texture(false), "textures\\terminals\\pc\\BBRTOn.dds");
        assert_eq!(shade("P1_PointVal_04_GLOW:4", true), BULB_HALO_TINT);
        assert_eq!(shade("P1_PointVal_04_GLOW:5", true), BULB_LIT_TINT);
        assert_eq!(shade("P1_PointVal_04:3", true), BULB_DULL_TINT);
        assert_eq!(shade("Cabinet:0", true), CABINET_TINT);
        assert_eq!(shade("S_01:0", false), BOARD_TINT);
        for [halo, lit] in (1..=10).map(glow_shapes) {
            assert!(is_glow(&halo) && is_glow(&lit));
        }
        assert!(!is_glow("P1_PointVal_04:3"));
    }

    #[test]
    fn points_go_where_the_page_says() {
        let mut v = Vigor::new(fives());
        assert_eq!(v.act(Action::Raise(None)), Outcome::Refused("no attribute on this page"));
        assert_eq!(v.act(Action::Next), Outcome::Turned { from: Page::Attract, to: Page::Stat(0) });
        assert_eq!(v.act(Action::Raise(None)), Outcome::Changed { stat: 0, value: 6, remaining: 4 });
        // On an attribute's page only that attribute's buttons.
        assert_eq!(v.act(Action::Raise(Some(3))), Outcome::Refused("no attribute on this page"));
        for _ in 0..4 {
            v.act(Action::Raise(None));
        }
        assert_eq!(v.sheet.values[0], 10);
        assert_eq!(v.act(Action::Raise(None)), Outcome::Refused("at 10"));
        assert_eq!(v.act(Action::Done), Outcome::Done);
        v.act(Action::Lower(None));
        assert_eq!(v.act(Action::Done), Outcome::Refused("points still to give out"));
        // To the review page: rows, and → is Done only when complete.
        while v.page != Page::Review {
            v.act(Action::Next);
        }
        assert_eq!(v.act(Action::Next), Outcome::Refused("points still to give out"));
        assert_eq!(v.act(Action::Step(3)), Outcome::Selected(3));
        assert_eq!(v.act(Action::Raise(None)), Outcome::Changed { stat: 3, value: 6, remaining: 0 });
        assert_eq!(v.act(Action::Raise(Some(5))), Outcome::Refused("no points left"));
        assert_eq!(v.row, 5);
        assert_eq!(v.act(Action::Lower(Some(6))), Outcome::Changed { stat: 6, value: 4, remaining: 1 });
        v.act(Action::Raise(Some(6)));
        assert_eq!(v.act(Action::Next), Outcome::Done);
        assert_eq!(v.values(), "9 5 5 6 5 5 5");
        // Back down to the first page, no further.
        while v.page != Page::Attract {
            v.act(Action::Prev);
        }
        assert_eq!(v.act(Action::Prev), Outcome::Refused("first page"));
        let mut w = Vigor::new(SpecialSheet::new([1; 7]));
        w.page = Page::Review;
        assert_eq!(w.act(Action::Lower(Some(2))), Outcome::Refused("at 1"));
    }

    #[test]
    fn clicks_find_their_action() {
        assert_eq!(button_action("P1_RT_Btn:0", Page::Stat(2)), Some(Action::Next));
        assert_eq!(button_action("LookInside_Btn:0", Page::Attract), Some(Action::Next));
        assert_eq!(button_action("P1_Increase_Btn:0", Page::Stat(2)), Some(Action::Raise(None)));
        assert_eq!(button_action("Index_AgilityIncrease_Btn:0", Page::Review), Some(Action::Raise(Some(5))));
        assert_eq!(button_action("Index_LuckDecrease_Btn:0", Page::Review), Some(Action::Lower(Some(6))));
        assert_eq!(button_action("Index_IntelligencePointVal:0", Page::Review), Some(Action::Select(4)));
        assert_eq!(button_action("Index_LuckDecrease_Btn:0", Page::Stat(6)), None);
        assert_eq!(live_buttons(Page::Attract), vec!["LookInside_Btn:0".to_string()]);
        assert_eq!(live_buttons(Page::Review).len(), 4 + 21);
    }

    #[test]
    fn the_aid_parses() {
        let a = parse_aid("4:page:next, 5:raise, 5.5:raise:luck, 6:lower:2, 7:select:A, 8:page:review, 9:click:P1_RT_Btn:0, 10:done").unwrap();
        assert_eq!(
            a,
            vec![
                (4.0, AidOp::Act(Action::Next)),
                (5.0, AidOp::Act(Action::Raise(None))),
                (5.5, AidOp::Act(Action::Raise(Some(6)))),
                (6.0, AidOp::Act(Action::Lower(Some(2)))),
                (7.0, AidOp::Act(Action::Select(5))),
                (8.0, AidOp::Page(Page::Review)),
                (9.0, AidOp::Click("P1_RT_Btn:0".into())),
                (10.0, AidOp::Act(Action::Done)),
            ]
        );
        assert!(parse_aid("3:raise:x").is_err());
        assert!(parse_aid("3:jump").is_err());
        assert!(parse_aid("3:page").is_err());
        assert_eq!(parse_stat("s"), Some(0));
        assert_eq!(parse_stat("7"), None);
    }
}
