//! The terminal hacking game (docs/spec-notes/hacking.md): a memory dump of two columns
//! of 17 rows of 12 characters, words of one length hidden among symbols, one of them
//! the password. The player picks a word and is told how many of its letters stand in
//! the password at the same place; bracket pairs remove a dud or give the attempts back.
//!
//! Pure rules, no drawing: the engine's terminal screen shows a [`Puzzle`] and passes
//! the player's picks to [`Puzzle::choose`]. The words come from the game's own word
//! list (`menus\falloutdict.txt`, read by the caller from the owner's archives).

use rand_core::RngCore;
use std::collections::BTreeMap;
use std::ops::RangeInclusive;

/// Two columns of 17 rows of 12 characters [observed: the owner's screenshots,
/// hacking.md §2; documented: `menus\hacking_menu.xml` gives a column 19 characters,
/// the address "0xF650" and a space being the other 7].
pub const COLUMNS: usize = 2;
pub const ROWS: usize = 17;
pub const ROW_CHARS: usize = 12;
pub const DUMP_CHARS: usize = COLUMNS * ROWS * ROW_CHARS;

/// Attempts the player starts with [observed: four on the attempts line, hacking.md §2].
pub const ATTEMPTS: u8 = 4;

/// The symbols between the words: every character other than a letter on the owner's
/// screenshots [observed, hacking.md §2]. `&`, `~` and the backquote were never seen.
pub const FILLER: &[u8] = b"!\"#$%'()*+,-./:;<=>?@[\\]^_{|}";

/// The character a removed dud's letters become [hypothesis, hacking.md §5].
pub const REMOVED: u8 = b'.';

/// Characters of one log line after its `>` [observed: the wait message is shown on
/// three lines of at most 12, broken between words, hacking.md §2].
pub const LOG_CHARS: usize = 12;

/// The log's words. None is in the plugins or the menu files, so they are NVForever's
/// own (LEGAL.md, hacking.md §8); only where each appears follows the original.
pub mod words {
    /// After the password.
    pub const GRANTED: &str = "Accepted.";
    /// After [`GRANTED`], while the terminal's menu comes up.
    pub const WAIT: &str = "Opening the terminal...";
    /// After a wrong word.
    pub const DENIED: &str = "Not accepted";
    /// When a bracket pair gives the attempts back.
    pub const REPLENISHED: &str = "Tries restored.";
    /// When a bracket pair takes a dud out [hypothesis, hacking.md §5].
    pub const DUD_REMOVED: &str = "Dud removed.";
    /// A symbol that is neither a word nor a bracket pair [hypothesis, hacking.md §5].
    pub const ERROR: &str = "Error";

    /// After [`DENIED`]: how many letters of the guess stand where the password has them.
    pub fn likeness(n: usize, len: usize) -> String {
        format!("{n} of {len} right")
    }
}

/// The game's word list, by length: upper-case words of letters only.
#[derive(Debug, Clone, Default)]
pub struct Dictionary {
    by_len: BTreeMap<usize, Vec<String>>,
}

impl Dictionary {
    /// Read `menus\falloutdict.txt`: one line of upper-case words separated by spaces
    /// [observed: 4,710 words, 4,709 different, letters only, hacking.md §3]. Anything
    /// that is not a word of letters is left out; each word is kept once.
    pub fn parse(text: &str) -> Dictionary {
        let mut by_len: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        let mut seen = std::collections::HashSet::new();
        for w in text.split_whitespace() {
            if !w.bytes().all(|b| b.is_ascii_alphabetic()) {
                continue;
            }
            let w = w.to_ascii_uppercase();
            if seen.insert(w.clone()) {
                by_len.entry(w.len()).or_default().push(w);
            }
        }
        Dictionary { by_len }
    }

    pub fn words(&self, len: usize) -> &[String] {
        self.by_len.get(&len).map_or(&[], Vec::as_slice)
    }

    pub fn len(&self) -> usize {
        self.by_len.values().map(Vec::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.by_len.is_empty()
    }
}

/// A terminal's hacking difficulty: TERM DNAM "Base Hacking Difficulty" 0 to 5
/// [documented: xEdit wbDefinitionsFNV.pas; GECK wiki GetLockLevel, terminal values].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Difficulty {
    VeryEasy,
    Easy,
    Average,
    Hard,
    VeryHard,
    /// Cannot be hacked; only its password note opens it.
    RequiresKey,
}

impl Difficulty {
    pub fn from_code(code: u8) -> Difficulty {
        match code {
            0 => Difficulty::VeryEasy,
            1 => Difficulty::Easy,
            2 => Difficulty::Average,
            3 => Difficulty::Hard,
            4 => Difficulty::VeryHard,
            _ => Difficulty::RequiresKey,
        }
    }

    pub fn code(self) -> u8 {
        self as u8
    }

    pub fn name(self) -> &'static str {
        match self {
            Difficulty::VeryEasy => "Very Easy",
            Difficulty::Easy => "Easy",
            Difficulty::Average => "Average",
            Difficulty::Hard => "Hard",
            Difficulty::VeryHard => "Very Hard",
            Difficulty::RequiresKey => "Requires Key",
        }
    }

    /// The Science skill the level stands for: the level's code times `fHackLevelMult`
    /// (0.25) times 100, so 0, 25, 50, 75, 100 [documented: GECK wiki "Hacking Word
    /// Count"; the owner, 2026-10-11]; none for Requires Key.
    pub fn lock_level(self, s: &Settings) -> Option<f32> {
        (self != Difficulty::RequiresKey).then(|| self.code() as f32 * s.hack_level_mult * 100.0)
    }

    /// The length of the words [hypothesis, hacking.md §4: the lengths players report
    /// for Fallout 3 and New Vegas; the owner's Average-or-easier screenshot has words of
    /// 7]. Not checked against the game per level yet.
    pub fn word_lengths(self) -> RangeInclusive<usize> {
        match self {
            Difficulty::VeryEasy => 4..=5,
            Difficulty::Easy => 6..=8,
            Difficulty::Average => 9..=10,
            Difficulty::Hard => 11..=12,
            Difficulty::VeryHard | Difficulty::RequiresKey => 13..=15,
        }
    }
}

/// The game settings the word count uses. None of them is in the game's plugins; these
/// are the defaults the GECK wiki gives [documented: "Hacking Word Count"].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    pub min_words: u32,
    pub max_words: u32,
    pub hack_level_mult: f32,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { min_words: 5, max_words: 20, hack_level_mult: 0.25 }
    }
}

/// Why the player cannot hack a terminal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Refusal {
    /// Requires Key: only the password note opens it.
    RequiresKey,
    /// The player's Science is below the terminal's level [hypothesis, hacking.md §4].
    Science { needed: f32 },
}

/// Whether a player of this Science skill may hack at all.
pub fn may_hack(science: f32, difficulty: Difficulty, s: &Settings) -> Result<(), Refusal> {
    match difficulty.lock_level(s) {
        None => Err(Refusal::RequiresKey),
        Some(needed) if science < needed => Err(Refusal::Science { needed }),
        Some(_) => Ok(()),
    }
}

/// How many words the dump holds: `(1 - (Science - lock) / (100 - lock)) * (max - min) +
/// min`, the fraction replaced by 0.5 where `100 - lock` is 0 (Very Hard), and never more
/// than 20 [documented: GECK wiki "Hacking Word Count"]. Rounded down [hypothesis].
/// Science above 100 is taken as 100 [design: the formula would go below the least].
pub fn word_count(science: f32, difficulty: Difficulty, s: &Settings) -> u32 {
    let lock = difficulty.lock_level(s).unwrap_or(100.0);
    let lock_offset = 100.0 - lock;
    let frac = if lock_offset <= 0.0 { 0.5 } else { ((science.min(100.0) - lock) / lock_offset).clamp(0.0, 1.0) };
    let n = (1.0 - frac) * (s.max_words as f32 - s.min_words as f32) + s.min_words as f32;
    (n.floor().max(1.0) as u32).min(20)
}

/// Letters of `guess` that stand in `password` at the same place [observed: LOOTING
/// against MEETING shows a likeness of 4 of 7, the shared "TING", hacking.md §2].
pub fn likeness(guess: &str, password: &str) -> usize {
    guess.bytes().zip(password.bytes()).filter(|(a, b)| a == b).count()
}

/// A word in the dump.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub text: String,
    /// Index of its first character in the dump (it may run on into the next row).
    pub start: usize,
    /// A dud taken out by a bracket pair: its letters are now [`REMOVED`].
    pub removed: bool,
}

impl Word {
    pub fn range(&self) -> std::ops::Range<usize> {
        self.start..self.start + self.text.len()
    }
}

/// What lies under one character of the dump.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// A word still in the dump (index into [`Puzzle::words`]).
    Word(usize),
    /// An opening bracket and its closing one in the same row, with no letter between,
    /// that has not been used: `start..=end`.
    Brackets { start: usize, end: usize },
    /// Any other character.
    Char(usize),
}

/// What a pick did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// A wrong word: one attempt gone; `likeness` letters right.
    Denied { likeness: usize },
    /// The password: the terminal opens.
    Granted,
    /// A wrong word on the last attempt: the terminal locks for good.
    LockedOut,
    DudRemoved,
    Replenished,
    /// A symbol that is neither a word nor a bracket pair [hypothesis, hacking.md §5].
    Error,
    /// Nothing happens: the game is over.
    Ignored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Guessing,
    Granted,
    LockedOut,
}

/// One game of hacking.
#[derive(Debug, Clone)]
pub struct Puzzle {
    /// The dump, column 1 then column 2, each row after row.
    pub chars: Vec<u8>,
    /// The address of the first row; each row is 12 on [observed: 0xF650, 0xF65C, …;
    /// column 2 goes on from column 1, 0xF71C].
    pub address: u16,
    pub words: Vec<Word>,
    /// Index of the password in `words`.
    pub password: usize,
    pub attempts: u8,
    /// Opening brackets already used.
    pub used: Vec<usize>,
    /// The log at the right, oldest first, each line with its `>`.
    pub log: Vec<String>,
    pub state: State,
}

fn below(rng: &mut impl RngCore, n: usize) -> usize {
    if n == 0 {
        0
    } else {
        (rng.next_u64() % n as u64) as usize
    }
}

fn shuffle<T>(v: &mut [T], rng: &mut impl RngCore) {
    for i in (1..v.len()).rev() {
        v.swap(i, below(rng, i + 1));
    }
}

fn closing(open: u8) -> Option<u8> {
    match open {
        b'(' => Some(b')'),
        b'[' => Some(b']'),
        b'{' => Some(b'}'),
        b'<' => Some(b'>'),
        _ => None,
    }
}

/// Split a text into log lines of at most [`LOG_CHARS`] after a `>`, breaking between
/// words [observed, hacking.md §2].
pub fn log_lines(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut line = String::new();
    for w in text.split_whitespace() {
        if !line.is_empty() && line.len() + 1 + w.len() > LOG_CHARS {
            out.push(format!(">{line}"));
            line.clear();
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(w);
    }
    if !line.is_empty() || out.is_empty() {
        out.push(format!(">{line}"));
    }
    out
}

impl Puzzle {
    /// A new game: `count` words of one length the difficulty allows, the password one
    /// of them. `None` when the dictionary has too few words for every length allowed.
    pub fn new(dict: &Dictionary, difficulty: Difficulty, count: u32, rng: &mut impl RngCore) -> Option<Puzzle> {
        let count = (count.max(1) as usize).min(20);
        // [design] A length is drawn among those allowed that have enough words; the
        // longest allowed lengths have few (three words of 15 letters).
        let lengths: Vec<usize> = difficulty.word_lengths().filter(|&l| dict.words(l).len() > count * 2).collect();
        let len = *lengths.get(below(rng, lengths.len()))?;
        let pool = dict.words(len);
        let password = pool[below(rng, pool.len())].clone();
        let duds = pick_duds(pool, &password, count - 1, rng);
        let mut texts = duds;
        texts.push(password.clone());
        shuffle(&mut texts, rng);
        let chars_and_words = place(&texts, rng)?;
        let (chars, words) = chars_and_words;
        let password = words.iter().position(|w| w.text == password)?;
        // [design] A four-digit address that the whole dump stays within.
        let address = 0x1000 + below(rng, 0xF000 - DUMP_CHARS) as u16;
        Some(Puzzle { chars, address, words, password, attempts: ATTEMPTS, used: Vec::new(), log: Vec::new(), state: State::Guessing })
    }

    /// The address shown before a row.
    pub fn row_address(&self, column: usize, row: usize) -> u16 {
        self.address.wrapping_add(((column * ROWS + row) * ROW_CHARS) as u16)
    }

    /// The dump index of a character on the screen.
    pub fn index(column: usize, row: usize, col: usize) -> usize {
        (column * ROWS + row) * ROW_CHARS + col
    }

    pub fn row_text(&self, column: usize, row: usize) -> &str {
        let i = Self::index(column, row, 0);
        std::str::from_utf8(&self.chars[i..i + ROW_CHARS]).unwrap_or("")
    }

    pub fn password(&self) -> &str {
        &self.words[self.password].text
    }

    /// The bracket pair that starts at `index`, if any: the first closing bracket of the
    /// same kind later in the same row, with no letter before it [hypothesis, hacking.md
    /// §5; observed: "{[_}" in the owner's screenshot was one pair].
    pub fn pair_at(&self, index: usize) -> Option<usize> {
        let close = closing(*self.chars.get(index)?)?;
        let row_end = (index / ROW_CHARS + 1) * ROW_CHARS;
        for j in index + 1..row_end {
            let c = self.chars[j];
            if c.is_ascii_alphabetic() {
                return None;
            }
            if c == close {
                return Some(j);
            }
        }
        None
    }

    /// What the character at `index` belongs to.
    pub fn target(&self, index: usize) -> Target {
        if let Some(w) = self.words.iter().position(|w| !w.removed && w.range().contains(&index)) {
            return Target::Word(w);
        }
        if !self.used.contains(&index) {
            if let Some(end) = self.pair_at(index) {
                return Target::Brackets { start: index, end };
            }
        }
        Target::Char(index)
    }

    /// The characters a target covers (lit up under the pointer).
    pub fn span(&self, t: Target) -> std::ops::Range<usize> {
        match t {
            Target::Word(w) => self.words[w].range(),
            Target::Brackets { start, end } => start..end + 1,
            Target::Char(i) => i..i + 1,
        }
    }

    /// The text a target enters at the prompt.
    pub fn text(&self, t: Target) -> String {
        String::from_utf8_lossy(&self.chars[self.span(t)]).into_owned()
    }

    /// Every unused bracket pair now in the dump.
    pub fn pairs(&self) -> Vec<(usize, usize)> {
        (0..DUMP_CHARS).filter(|i| !self.used.contains(i)).filter_map(|i| self.pair_at(i).map(|e| (i, e))).collect()
    }

    fn say(&mut self, text: &str) {
        self.log.extend(log_lines(text));
    }

    /// The player picks the character at `index`.
    pub fn choose(&mut self, index: usize, rng: &mut impl RngCore) -> Outcome {
        if self.state != State::Guessing || index >= DUMP_CHARS {
            return Outcome::Ignored;
        }
        let t = self.target(index);
        let entered = self.text(t);
        self.say(&entered);
        match t {
            Target::Word(w) if w == self.password => {
                self.state = State::Granted;
                self.say(words::GRANTED);
                self.say(words::WAIT);
                Outcome::Granted
            }
            Target::Word(w) => {
                let n = likeness(&self.words[w].text, self.password());
                self.attempts -= 1;
                self.say(words::DENIED);
                self.say(&words::likeness(n, self.password().len()));
                if self.attempts == 0 {
                    self.state = State::LockedOut;
                    Outcome::LockedOut
                } else {
                    Outcome::Denied { likeness: n }
                }
            }
            Target::Brackets { start, .. } => {
                self.used.push(start);
                let duds: Vec<usize> = (0..self.words.len()).filter(|&w| w != self.password && !self.words[w].removed).collect();
                // [hypothesis, hacking.md §5] One pair in five gives the attempts back;
                // the others remove a dud, or give the attempts back once no dud is left.
                if duds.is_empty() || below(rng, 5) == 0 {
                    self.attempts = ATTEMPTS;
                    self.say(words::REPLENISHED);
                    Outcome::Replenished
                } else {
                    let w = duds[below(rng, duds.len())];
                    self.words[w].removed = true;
                    for i in self.words[w].range() {
                        self.chars[i] = REMOVED;
                    }
                    self.say(words::DUD_REMOVED);
                    Outcome::DudRemoved
                }
            }
            Target::Char(_) => {
                self.say(words::ERROR);
                Outcome::Error
            }
        }
    }
}

/// The duds: words of the password's length, none the password, each sharing at least
/// one letter in place with it, spread evenly over how many they share [design, fitted
/// to the owner's screenshot: the 19 duds shared 1 to 5 of 7 letters, 3 to 5 at each
/// count]. Words sharing none fill in when there are too few.
fn pick_duds(pool: &[String], password: &str, n: usize, rng: &mut impl RngCore) -> Vec<String> {
    let len = password.len();
    let mut buckets: Vec<Vec<&String>> = vec![Vec::new(); len];
    for w in pool {
        let l = likeness(w, password);
        if l < len {
            buckets[l].push(w);
        }
    }
    for b in &mut buckets {
        shuffle(b, rng);
    }
    let mut out: Vec<String> = Vec::with_capacity(n);
    while out.len() < n {
        let open: Vec<usize> = (1..len).filter(|&l| !buckets[l].is_empty()).collect();
        let pick = if open.is_empty() {
            match buckets[0].pop() {
                Some(w) => w,
                None => break,
            }
        } else {
            let l = open[below(rng, open.len())];
            buckets[l].pop().expect("non-empty")
        };
        out.push(pick.clone());
    }
    out
}

/// Lay the words out in the dump: the dump is cut into as many equal stretches as there
/// are words and each word placed at random within its own, with at least one symbol
/// before and after it; everything else random symbols [design: the owner's screenshot
/// shows the words about evenly spread, about one in every 20 characters with 20 words,
/// and running on across rows and from column 1 into column 2].
fn place(words: &[String], rng: &mut impl RngCore) -> Option<(Vec<u8>, Vec<Word>)> {
    let mut chars: Vec<u8> = (0..DUMP_CHARS).map(|_| FILLER[below(rng, FILLER.len())]).collect();
    let n = words.len();
    let stretch = DUMP_CHARS / n.max(1);
    let mut out = Vec::with_capacity(n);
    for (k, w) in words.iter().enumerate() {
        let len = w.len();
        if len + 2 > stretch {
            return None;
        }
        let start = k * stretch + 1 + below(rng, stretch - len - 1);
        chars[start..start + len].copy_from_slice(w.as_bytes());
        out.push(Word { text: w.clone(), start, removed: false });
    }
    Some((chars, out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    /// The words of the owner's screenshot (password MEETING) and a few of other lengths.
    const SHOT: &str = "GETTING JUSTICE FORTIFY CANTINA TESTING PUTTING SLIDING SLITHER SEEKING SPOTTED \
        STATING STAMINA LOOTING SHOTGUN SOLDIER FLOWING CONTENT BLOWING MEETING HURTING";

    fn dict() -> Dictionary {
        let mut text = String::from(SHOT);
        // Enough words of every length 4 to 15 for any difficulty.
        for len in 4..=15 {
            for i in 0..60u32 {
                // Two letters spell the number, the rest vary with it and the place.
                let w: String = (0..len)
                    .map(|k| match k {
                        0 => b'A' + (i / 26) as u8,
                        1 => b'A' + (i % 26) as u8,
                        _ => b'A' + ((i * 7 + k as u32 * 3 + len as u32) % 26) as u8,
                    } as char)
                    .collect();
                text.push(' ');
                text.push_str(&w);
            }
        }
        Dictionary::parse(&text)
    }

    #[test]
    fn the_screenshots_guess_scores_four_of_seven() {
        assert_eq!(likeness("LOOTING", "MEETING"), 4);
        assert_eq!(likeness("MEETING", "MEETING"), 7);
        // The screenshot's 19 duds share 1 to 5 letters with the password, none 0.
        let counts: Vec<usize> = SHOT.split(' ').filter(|w| *w != "MEETING").map(|w| likeness(w, "MEETING")).collect();
        assert_eq!(counts.iter().min(), Some(&1));
        assert_eq!(counts.iter().max(), Some(&5));
    }

    #[test]
    fn the_dictionary_keeps_letter_words_once() {
        let d = Dictionary::parse("STORY THE PC PC 2077 STORY-LINE hello\nWORLD");
        assert_eq!(d.words(5), &["STORY".to_string(), "HELLO".into(), "WORLD".into()]);
        assert_eq!(d.words(2), &["PC".to_string()]);
        assert_eq!(d.len(), 5);
    }

    #[test]
    fn the_word_count_follows_the_wiki_formula() {
        let s = Settings::default();
        // Science equal to the level: the most words; 100 Science on an Average
        // terminal: the least.
        assert_eq!(word_count(50.0, Difficulty::Average, &s), 20);
        assert_eq!(word_count(100.0, Difficulty::Average, &s), 5);
        assert_eq!(word_count(75.0, Difficulty::Average, &s), 12);
        assert_eq!(word_count(15.0, Difficulty::VeryEasy, &s), 17);
        // Very Hard: the fraction is 0.5 whatever the skill.
        assert_eq!(word_count(100.0, Difficulty::VeryHard, &s), 12);
        assert_eq!(Difficulty::Hard.lock_level(&s), Some(75.0));
        assert_eq!(Difficulty::RequiresKey.lock_level(&s), None);
    }

    #[test]
    fn science_below_the_level_or_a_key_terminal_refuses() {
        let s = Settings::default();
        assert_eq!(may_hack(49.0, Difficulty::Average, &s), Err(Refusal::Science { needed: 50.0 }));
        assert_eq!(may_hack(50.0, Difficulty::Average, &s), Ok(()));
        assert_eq!(may_hack(100.0, Difficulty::RequiresKey, &s), Err(Refusal::RequiresKey));
        assert_eq!(Difficulty::from_code(9), Difficulty::RequiresKey);
    }

    #[test]
    fn log_lines_wrap_as_the_screenshot() {
        assert_eq!(log_lines(words::WAIT), vec![">Opening the", ">terminal..."]);
        assert_eq!(log_lines(&words::likeness(4, 7)), vec![">4 of 7 right"]);
        assert_eq!(log_lines(words::DENIED), vec![">Not accepted"]);
        assert_eq!(log_lines(words::REPLENISHED), vec![">Tries", ">restored."]);
    }

    #[test]
    fn a_new_puzzle_hides_its_words_in_the_dump() {
        let d = dict();
        for seed in 0..50 {
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let p = Puzzle::new(&d, Difficulty::Easy, 20, &mut rng).expect("puzzle");
            assert_eq!(p.chars.len(), DUMP_CHARS);
            assert_eq!(p.words.len(), 20);
            let len = p.password().len();
            assert!((6..=8).contains(&len));
            for (i, w) in p.words.iter().enumerate() {
                assert_eq!(w.text.len(), len);
                assert_eq!(&p.chars[w.range()], w.text.as_bytes());
                // At least one symbol either side; words never touch.
                assert!(!p.chars[w.start - 1].is_ascii_alphabetic());
                assert!(!p.chars[w.range().end].is_ascii_alphabetic());
                if i != p.password {
                    assert_ne!(w.text, p.password());
                }
            }
            // Every letter of the dump belongs to a word.
            let letters = p.chars.iter().filter(|c| c.is_ascii_alphabetic()).count();
            assert_eq!(letters, 20 * len);
            assert!(p.chars.iter().all(|c| c.is_ascii_alphabetic() || FILLER.contains(c)));
            assert!(p.row_address(1, 16) as usize + ROW_CHARS <= 0x10000);
        }
    }

    #[test]
    fn rows_and_addresses_run_on_from_column_one_into_column_two() {
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let mut p = Puzzle::new(&dict(), Difficulty::Easy, 20, &mut rng).unwrap();
        p.address = 0xF650;
        assert_eq!(p.row_address(0, 1), 0xF65C);
        assert_eq!(p.row_address(0, 16), 0xF710);
        assert_eq!(p.row_address(1, 0), 0xF71C);
        assert_eq!(Puzzle::index(1, 0, 0), 17 * 12);
        assert_eq!(p.row_text(1, 0).len(), 12);
    }

    /// A puzzle laid out by hand: the screenshot's row "\.,@\:|{[_})" and words.
    fn by_hand() -> Puzzle {
        let mut chars = vec![b'.'; DUMP_CHARS];
        chars[..12].copy_from_slice(b"\\.,@\\:|{[_})");
        chars[12..24].copy_from_slice(b"+(LOOTING%.)");
        chars[24..36].copy_from_slice(b"<.MEETING.>.");
        chars[36..48].copy_from_slice(b"..GETTING...");
        let words = vec![
            Word { text: "LOOTING".into(), start: 14, removed: false },
            Word { text: "MEETING".into(), start: 26, removed: false },
            Word { text: "GETTING".into(), start: 38, removed: false },
        ];
        Puzzle { chars, address: 0xF6EC, words, password: 1, attempts: ATTEMPTS, used: Vec::new(), log: Vec::new(), state: State::Guessing }
    }

    #[test]
    fn brackets_pair_within_a_row_without_letters_between() {
        let p = by_hand();
        // "{[_}" is a pair; "[" has no "]" in its row; ")" opens nothing.
        assert_eq!(p.target(7), Target::Brackets { start: 7, end: 10 });
        assert_eq!(p.text(p.target(7)), "{[_}");
        assert_eq!(p.target(8), Target::Char(8));
        assert_eq!(p.target(11), Target::Char(11));
        // "(LOOTING%.)": a word stands between, so no pair; "<.MEETING.>" neither.
        assert_eq!(p.target(13), Target::Char(13));
        assert_eq!(p.target(24), Target::Char(24));
        // Any letter of a word is the word.
        assert_eq!(p.target(20), Target::Word(0));
        assert_eq!(p.text(Target::Word(0)), "LOOTING");
        assert_eq!(p.pairs(), vec![(7, 10)]);
    }

    #[test]
    fn a_wrong_word_costs_an_attempt_and_says_how_close() {
        let mut p = by_hand();
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        assert_eq!(p.choose(14, &mut rng), Outcome::Denied { likeness: 4 });
        assert_eq!(p.attempts, 3);
        assert_eq!(p.log, vec![">LOOTING", ">Not accepted", ">4 of 7 right"]);
        // A symbol: an error, no attempt lost.
        assert_eq!(p.choose(0, &mut rng), Outcome::Error);
        assert_eq!(p.attempts, 3);
        assert_eq!(&p.log[3..], &[">\\".to_string(), ">Error".into()]);
    }

    #[test]
    fn the_password_opens_the_terminal() {
        let mut p = by_hand();
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        assert_eq!(p.choose(30, &mut rng), Outcome::Granted);
        assert_eq!(p.state, State::Granted);
        assert_eq!(p.log, vec![">MEETING", ">Accepted.", ">Opening the", ">terminal..."]);
        assert_eq!(p.choose(14, &mut rng), Outcome::Ignored);
    }

    #[test]
    fn four_wrong_words_lock_the_terminal() {
        let mut p = by_hand();
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        for _ in 0..3 {
            assert!(matches!(p.choose(40, &mut rng), Outcome::Denied { likeness: 5 }));
        }
        assert_eq!(p.choose(40, &mut rng), Outcome::LockedOut);
        assert_eq!((p.attempts, p.state), (0, State::LockedOut));
        assert_eq!(p.choose(30, &mut rng), Outcome::Ignored);
    }

    #[test]
    fn a_bracket_pair_works_once() {
        let mut seen_dud = false;
        let mut seen_refill = false;
        for seed in 0..40 {
            let mut p = by_hand();
            p.attempts = 2;
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            match p.choose(7, &mut rng) {
                Outcome::DudRemoved => {
                    seen_dud = true;
                    let gone: Vec<&Word> = p.words.iter().filter(|w| w.removed).collect();
                    assert_eq!(gone.len(), 1);
                    assert_ne!(gone[0].text, "MEETING");
                    assert!(p.chars[gone[0].range()].iter().all(|&c| c == REMOVED));
                    assert_eq!(p.log.last().map(String::as_str), Some(">Dud removed."));
                    assert_eq!(p.attempts, 2);
                }
                Outcome::Replenished => {
                    seen_refill = true;
                    assert_eq!(p.attempts, ATTEMPTS);
                    assert_eq!(&p.log[1..], &[">Tries".to_string(), ">restored.".into()]);
                }
                o => panic!("{o:?}"),
            }
            assert_eq!(p.log[0], ">{[_}");
            // Used: now only its first character.
            assert_eq!(p.target(7), Target::Char(7));
        }
        assert!(seen_dud && seen_refill);
    }

    #[test]
    fn with_no_dud_left_a_pair_gives_the_attempts_back() {
        let mut p = by_hand();
        p.words[0].removed = true;
        p.words[2].removed = true;
        p.attempts = 1;
        let mut rng = ChaCha8Rng::seed_from_u64(3);
        assert_eq!(p.choose(7, &mut rng), Outcome::Replenished);
        assert_eq!(p.attempts, ATTEMPTS);
    }

    #[test]
    fn every_difficulty_makes_a_puzzle() {
        let d = dict();
        let s = Settings::default();
        let mut rng = ChaCha8Rng::seed_from_u64(9);
        for code in 0..5 {
            let diff = Difficulty::from_code(code);
            let science = diff.lock_level(&s).unwrap();
            let p = Puzzle::new(&d, diff, word_count(science, diff, &s), &mut rng).expect("puzzle");
            assert!(diff.word_lengths().contains(&p.password().len()));
        }
    }
}
