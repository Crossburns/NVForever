//! Slot machines (docs/spec-notes/slots.md): a casino's slot machine, activated by the
//! player, runs `ShowSlotMachineMenuParams <casino> 1 <max> 0`; when the player carries the
//! casino's chips and is not banned there (`fnv_game::casino::opening`) the slot machine
//! menu opens, else a line says why. Increase and Decrease Bet change the bet, Bet Max
//! makes it the largest; Spin takes the bet and turns the three reels (laid out from the
//! casino record's reel stops, `fnv_game::slots::reel`); when the last reel stops the
//! payline is paid (`fnv_game::slots::multiple`), the net goes to the player's earnings at
//! the casino and its winnings level (`fnv_game::casino::add_earnings`). At the casino's
//! max winnings the player is banned: the menu closes after the spin. Payout List shows
//! what each line pays.
//!
//! The first time the menu opens the help message `HelpSlots` comes up over it, as
//! lockpicking's (`crate::lockpick::help`).
//!
//! It is a `PlayState::Menu`: the world and the game clock stand still. To scripts it is
//! menu [`fnv_game::slots::MENU_CODE`]. The menu's text and buttons follow
//! `menus\slot_machine_menu.xml`; the reels are drawn flat from the record's symbol
//! pictures (`screen`).
//!
//! Keys [documented: the menu file's `_PCButton_W/E/Q/F/S/R`]: W spins, E and Q increase
//! and decrease the bet, F shows the payout list, S bets the most, R leaves (Esc and Tab
//! too, Enter spins [design]). The buttons can be clicked.
//!
//! Test aid `NVF_PLAY_TEST_SLOTS=<seconds>:<step>[,…]`: `chips:<n>[:<casino hex>]` gives the
//! player that many of the casino's chips (The Tops' when left out), `open[:<casino hex>]`
//! as if a machine's script asked (min 1, max 75), `seed:<n>` (before `open`), `bet:<n>`
//! sets the bet, `up`, `down`, `max`, `reels:<a>-<b>-<c>` makes the next spin stop on those
//! symbols (1 to 7, 7 the cherries W; slots.md §2), `spin`, `payout` shows or hides the
//! payout list, `help` closes the help message, `exit` leaves. At exit, when the menu was
//! asked for: "slots summary: …".

pub mod screen;

use crate::lockpick::Help;
use crate::pause::{AidTime, Paused};
use crate::script::ScriptState;
use crate::sfx::PlaySound;
use crate::sim::SimState;
use crate::{PlaySet, PlayState};
use bevy::prelude::*;
use crate::ui_pointer::PointerState;
use fnv_game::casino;
use fnv_game::slots::{self as rules, Limits, REELS};
use fnv_sim::FormId;
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// The menu code scripts see.
pub const MENU_SLOTS: u16 = rules::MENU_CODE;
/// The Tops' casino record, the test aid's default [observed: FalloutNV.esm CSNO
/// `TheTopsCasinoData`].
pub const THE_TOPS: u32 = 0x0014_4C3F;
/// When each reel stops after the spin starts, seconds [design].
pub const REEL_STOPS: [f32; REELS] = [1.4, 2.0, 2.6];
/// Whole turns each reel makes before it stops [design].
pub const REEL_TURNS: [usize; REELS] = [2, 3, 4];
/// How long the result stays on the screen before the next spin can start [design].
pub const RESULT_SECONDS: f32 = 2.0;
/// How long a request waits for the player to be back in the world, seconds [design].
pub const REQUEST_WAIT: f32 = 3.0;

/// The slot machine sounds [observed: FalloutNV.esm SOUN records named for them; design:
/// which event each is for, slots.md §5].
pub mod sounds {
    use fnv_sim::FormId;
    pub const ACTIVATE: FormId = FormId(0x0014_D3EB);
    pub const PULL: FormId = FormId(0x0014_89D3);
    pub const INCREASE: [FormId; 3] = [FormId(0x0014_89D4), FormId(0x0014_89D5), FormId(0x0014_89D6)];
    pub const DECREASE: FormId = FormId(0x0014_89D7);
    pub const WIN_SMALL: FormId = FormId(0x0014_89D8);
    pub const WIN_MED: FormId = FormId(0x0014_89D9);
    pub const WIN_JACKPOT: FormId = FormId(0x0014_89DA);
    pub const LOSE: FormId = FormId(0x0014_89FA);
}

/// The open slot machine menu, if any.
#[derive(Resource, Default)]
pub struct SlotsScreen(pub Option<Session>);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Betting,
    /// The reels turn: since (aid seconds), where each started and the stop it comes to.
    Spinning { since: f32, from: [usize; REELS], to: [usize; REELS] },
    /// The line is paid; the result shows until `until` (aid seconds).
    Paid { until: f32 },
}

pub struct Session {
    pub casino: FormId,
    pub casino_name: String,
    pub chip: FormId,
    /// The symbols' pictures (CSNO ICON), under `textures\`, symbol 1 to W.
    pub symbols: Vec<String>,
    /// The reel's stops: the symbol (0 to 6) at each.
    pub reel: Vec<u8>,
    pub max_winnings: u32,
    pub limits: Limits,
    pub bet: u32,
    /// The stop each reel shows on the line.
    pub stops: [usize; REELS],
    pub phase: Phase,
    /// The chips the player carries.
    pub chips: u32,
    pub earnings: i64,
    /// The status line (slots.md §5).
    pub status: Option<String>,
    pub help: Option<Help>,
    /// The payout list is shown.
    pub payouts: bool,
    /// Banned by the last spin: the menu closes when its result has shown.
    pub banned: bool,
    /// The test aid's line for the next spin (symbols 0 to 6).
    forced: Option<[u8; REELS]>,
    increases: usize,
    rng: ChaCha8Rng,
}

impl Session {
    /// Where each reel stands, in stops (a fraction while it turns), at `now`.
    pub fn positions(&self, now: f32) -> [f32; REELS] {
        let n = self.reel.len().max(1);
        match self.phase {
            Phase::Spinning { since, from, to } => std::array::from_fn(|i| {
                let t = ((now - since) / REEL_STOPS[i]).clamp(0.0, 1.0);
                let ahead = (to[i] + n - from[i] % n) % n + REEL_TURNS[i] * n;
                // Slowing to the stop [design].
                let eased = 1.0 - (1.0 - t) * (1.0 - t);
                from[i] as f32 + ahead as f32 * eased
            }),
            _ => self.stops.map(|s| s as f32),
        }
    }

    /// The symbols on the line.
    pub fn line(&self) -> [u8; REELS] {
        self.stops.map(|s| self.reel.get(s % self.reel.len().max(1)).copied().unwrap_or(0))
    }

    pub fn betting(&self) -> bool {
        self.phase == Phase::Betting && self.help.is_none()
    }
}

/// A command for the open menu.
#[derive(Message, Clone, Debug, PartialEq)]
pub enum SlotsInput {
    Spin,
    Increase,
    Decrease,
    Max,
    Payouts,
    /// Test aid: the bet.
    Bet(u32),
    /// Test aid: the next spin's line (symbols 0 to 6).
    Line([u8; REELS]),
    CloseHelp,
    Exit,
}

#[derive(Resource, Default, Debug)]
struct SlotsStats {
    asked: u32,
    opened: u32,
    refused: u32,
    spins: u32,
    wins: u32,
    won: u64,
    lost: u64,
    bans: u32,
}

/// `NVF_PLAY_TEST_SLOTS`.
#[derive(Resource, Default)]
struct SlotsAid {
    steps: Vec<(f32, AidStep)>,
    seed: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
enum AidStep {
    Open(u32),
    Chips(u32, u32),
    Seed(u64),
    Input(SlotsInput),
}

fn hex(s: &str) -> Option<u32> {
    u32::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok()
}

/// `a-b-c`, symbols 1 to 7, as 0 to 6.
fn parse_line(s: &str) -> Option<[u8; REELS]> {
    let v: Vec<u8> = s.split('-').map(|x| x.trim().parse::<u8>().ok().filter(|n| (1..=rules::SYMBOLS as u8).contains(n)).map(|n| n - 1)).collect::<Option<_>>()?;
    v.try_into().ok()
}

fn parse_aid(v: &str) -> Result<Vec<(f32, AidStep)>, String> {
    crate::test_aid::timed(v)?
        .into_iter()
        .map(|(t, what)| {
            let (op, arg) = what.split_once(':').unwrap_or((what, ""));
            let num = |name: &str, s: &str| s.trim().parse::<u64>().map_err(|_| format!("{what:?}: expected {op}:<{name}>"));
            let step = match op {
                "open" if arg.is_empty() => AidStep::Open(THE_TOPS),
                "open" => AidStep::Open(hex(arg).ok_or_else(|| format!("{what:?}: expected open:<casino hex>"))?),
                "chips" => {
                    let (n, casino) = arg.split_once(':').unwrap_or((arg, ""));
                    let casino = if casino.is_empty() { THE_TOPS } else { hex(casino).ok_or_else(|| format!("{what:?}: expected chips:<n>:<casino hex>"))? };
                    AidStep::Chips(num("n", n)? as u32, casino)
                }
                "seed" => AidStep::Seed(num("n", arg)?),
                "bet" => AidStep::Input(SlotsInput::Bet(num("n", arg)? as u32)),
                "reels" => AidStep::Input(SlotsInput::Line(parse_line(arg).ok_or_else(|| format!("{what:?}: expected reels:<a>-<b>-<c>, symbols 1 to 7"))?)),
                "up" => AidStep::Input(SlotsInput::Increase),
                "down" => AidStep::Input(SlotsInput::Decrease),
                "max" => AidStep::Input(SlotsInput::Max),
                "payout" => AidStep::Input(SlotsInput::Payouts),
                "spin" => AidStep::Input(SlotsInput::Spin),
                "help" => AidStep::Input(SlotsInput::CloseHelp),
                "exit" => AidStep::Input(SlotsInput::Exit),
                _ => return Err(format!("{what:?}: expected open, chips:, seed:, bet:, up, down, max, reels:, spin, payout, help or exit")),
            };
            Ok((t, step))
        })
        .collect()
}

pub struct SlotsPlugin;

impl Plugin for SlotsPlugin {
    fn build(&self, app: &mut App) {
        let steps = match std::env::var("NVF_PLAY_TEST_SLOTS").map(|v| parse_aid(&v)) {
            Ok(Ok(a)) => a,
            Ok(Err(e)) => {
                warn!("NVF_PLAY_TEST_SLOTS: {e}");
                Vec::new()
            }
            Err(_) => Vec::new(),
        };
        app.init_resource::<SlotsScreen>()
            .init_resource::<SlotsStats>()
            .insert_resource(SlotsAid { steps, seed: None })
            .add_message::<SlotsInput>()
            .add_systems(
                Update,
                (run_aid, open, (keys, buttons, apply, step).chain().run_if(in_state(PlayState::Menu)), screen::draw)
                    .chain()
                    .after(crate::menu::open_menus)
                    .in_set(PlaySet::Ui),
            )
            .add_systems(OnExit(PlayState::Menu), close)
            .add_systems(Last, report);
    }
}

/// How many of `item` the player carries.
fn carried(sim: &SimState, item: FormId) -> u32 {
    sim.world.peek(FormId::PLAYER_REF).and_then(|s| s.inventory.as_ref()).map_or(0, |inv| inv.count(item)).max(0) as u32
}

fn run_aid(time: AidTime, mut aid: ResMut<SlotsAid>, mut sim: ResMut<SimState>, mut script: Option<ResMut<ScriptState>>, mut input: MessageWriter<SlotsInput>) {
    if aid.steps.is_empty() {
        return;
    }
    let Some(now) = time.world_secs() else { return };
    // Steps wait for the scripts (the casino records are theirs).
    if script.as_ref().and_then(|s| s.rt.as_ref()).is_none() {
        return;
    }
    let mut due: Vec<(f32, AidStep)> = aid.steps.iter().filter(|(t, _)| *t <= now).cloned().collect();
    due.sort_by(|a, b| a.0.total_cmp(&b.0));
    aid.steps.retain(|(t, _)| *t > now);
    for (_, step) in due {
        info!("test aid NVF_PLAY_TEST_SLOTS: {step:?}");
        match step {
            AidStep::Open(c) => {
                let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) else { continue };
                rt.game.slots_requests.push(rules::SlotsRequest { casino: FormId(c), min_bet: 1, max_bet: 75, max_winnings: 0 });
            }
            AidStep::Chips(n, c) => {
                let chip = script.as_ref().and_then(|s| s.rt.as_ref()).and_then(|rt| fnv_game::casino::chip(&rt.game, FormId(c)));
                match chip {
                    Some(chip) if n > 0 => match sim.world.add_item(FormId::PLAYER_REF, chip, n as i32) {
                        Ok(_) => info!("slots: the player is given {n} chips {chip} ({} carried)", carried(&sim, chip)),
                        Err(e) => warn!("slots: chips not given: {e:?}"),
                    },
                    _ => warn!("slots: no chip for casino {c:08X}"),
                }
            }
            AidStep::Seed(s) => aid.seed = Some(s),
            AidStep::Input(i) => {
                input.write(i);
            }
        }
    }
}

/// A seed: different every time unless the test aid fixed it.
fn seed(aid: &SlotsAid, casino: FormId, now: f32) -> u64 {
    aid.seed.unwrap_or_else(|| {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
        t ^ ((casino.0 as u64) << 20) ^ now.to_bits() as u64
    })
}

#[allow(clippy::too_many_arguments)]
fn open(
    state: Res<State<PlayState>>,
    mut next: ResMut<NextState<PlayState>>,
    mut screen: ResMut<SlotsScreen>,
    paused: Res<Paused>,
    sim: Res<SimState>,
    script: Option<ResMut<ScriptState>>,
    aid: Res<SlotsAid>,
    time: AidTime,
    (mut stats, mut corner, bindings, mut sounds): (ResMut<SlotsStats>, Option<ResMut<crate::corner_messages::CornerMessages>>, Res<crate::bindings::Bindings>, MessageWriter<PlaySound>),
    mut waiting: Local<Option<(rules::SlotsRequest, f32)>>,
) {
    let Some(rt) = script.and_then(|s| s.into_inner().rt.as_mut()) else { return };
    let now = time.elapsed_secs();
    if let Some(asked) = rt.game.slots_requests.pop() {
        rt.game.slots_requests.clear();
        stats.asked += 1;
        *waiting = Some((asked, now));
    }
    let Some((asked, since)) = *waiting else { return };
    // A request made while another menu closes waits for the world a moment [design].
    if *state.get() != PlayState::Playing || screen.0.is_some() || paused.0 {
        if now - since > REQUEST_WAIT {
            info!("slots: request for {} dropped, the player is not in the world", asked.casino);
            *waiting = None;
        }
        return;
    }
    *waiting = None;
    let Some(def) = rt.game.data.casinos.get(&asked.casino).cloned() else {
        warn!("slots: {} is not a casino", asked.casino);
        return;
    };
    let max = casino::max_winnings(&rt.game, asked.casino, asked.max_winnings);
    // A script's SetCasinoChip first (`fnv_game::casino::chip`).
    let def_chip = casino::chip(&rt.game, asked.casino);
    let chips = def_chip.map_or(0, |c| carried(&sim, c));
    if let Err(why) = casino::opening(&rt.game, asked.casino, chips, max) {
        stats.refused += 1;
        let line = casino::closed_line(why, &def.name);
        info!("slots: {} {} refused ({why:?}, {chips} chips): on screen: {line:?}", asked.casino, def.name);
        if let Some(c) = corner.as_deref_mut() {
            c.push(line.into());
        }
        return;
    }
    let Some(chip) = def_chip else { return };
    stats.opened += 1;
    let limits = Limits::new(asked.min_bet, asked.max_bet);
    let reel = rules::reel(&def.reel_stops);
    let earnings = rt.game.player.casino_earnings.get(&asked.casino).copied().unwrap_or(0);
    rt.game.open_menu(MENU_SLOTS);
    let help = rules::first_help(&mut rt.game).map(|(title, text)| Help::new(&rt.game, title, &text, &bindings));
    info!(
        "slots: open {} ({}): bets {} to {}, {chips} chips carried, earnings {earnings} of {max}, reel stops {:?} ({} a reel), {} symbol pictures, model {}{}",
        def.name,
        asked.casino,
        limits.min_bet,
        limits.max_bet,
        def.reel_stops,
        reel.len(),
        def.symbols.len(),
        def.slot_model.as_deref().unwrap_or("(none)"),
        if help.is_some() { "; help message shown (first use)" } else { "" }
    );
    sounds.write(PlaySound { sound: sounds::ACTIVATE, at: None });
    let mut rng = ChaCha8Rng::seed_from_u64(seed(&aid, asked.casino, now));
    let n = reel.len().max(1);
    let stops = std::array::from_fn(|_| (rand_core::RngCore::next_u32(&mut rng) as usize) % n);
    screen.0 = Some(Session {
        casino: asked.casino,
        casino_name: def.name.clone(),
        chip,
        symbols: def.symbols.clone(),
        reel,
        max_winnings: max,
        bet: limits.clamp(limits.min_bet, chips),
        limits,
        stops,
        phase: Phase::Betting,
        chips,
        earnings,
        status: None,
        help,
        payouts: false,
        banned: false,
        forced: None,
        increases: 0,
        rng,
    });
    next.set(PlayState::Menu);
}

fn keys(keys: Res<ButtonInput<KeyCode>>, screen: Res<SlotsScreen>, mut out: MessageWriter<SlotsInput>) {
    let Some(s) = screen.0.as_ref() else { return };
    if s.help.is_some() {
        if keys.any_just_pressed([KeyCode::KeyE, KeyCode::Escape, KeyCode::Tab, KeyCode::Enter]) {
            out.write(SlotsInput::CloseHelp);
        }
        return;
    }
    if keys.any_just_pressed([KeyCode::KeyR, KeyCode::Escape, KeyCode::Tab]) {
        out.write(SlotsInput::Exit);
        return;
    }
    let pairs = [
        (KeyCode::KeyW, SlotsInput::Spin),
        (KeyCode::Enter, SlotsInput::Spin),
        (KeyCode::KeyE, SlotsInput::Increase),
        (KeyCode::KeyQ, SlotsInput::Decrease),
        (KeyCode::KeyF, SlotsInput::Payouts),
        (KeyCode::KeyS, SlotsInput::Max),
    ];
    for (k, i) in pairs {
        if keys.just_pressed(k) {
            out.write(i);
        }
    }
}

fn buttons(
    screen: Res<SlotsScreen>,
    pressed: Query<(&PointerState, &screen::SlotsButton), Changed<PointerState>>,
    help_close: Query<(&PointerState, &crate::lockpick::screen::LockButton), Changed<PointerState>>,
    mut out: MessageWriter<SlotsInput>,
) {
    if screen.0.is_none() {
        return;
    }
    for (i, b) in &pressed {
        if *i == PointerState::Pressed {
            out.write(b.0.clone());
        }
    }
    if help_close.iter().any(|(i, _)| *i == PointerState::Pressed) {
        out.write(SlotsInput::CloseHelp);
    }
}

/// The win sound by what the line paid, times the bet (slots.md §5) [design].
fn win_sound(multiple: u32) -> FormId {
    match multiple {
        0 => sounds::LOSE,
        1..=5 => sounds::WIN_SMALL,
        6..=30 => sounds::WIN_MED,
        _ => sounds::WIN_JACKPOT,
    }
}

#[allow(clippy::too_many_arguments)]
fn apply(mut input: MessageReader<SlotsInput>, mut screen: ResMut<SlotsScreen>, mut sim: ResMut<SimState>, time: AidTime, mut stats: ResMut<SlotsStats>, mut next: ResMut<NextState<PlayState>>, mut sounds: MessageWriter<PlaySound>) {
    let now = time.elapsed_secs();
    let mut play = |sound| {
        sounds.write(PlaySound { sound, at: None });
    };
    for cmd in input.read() {
        let Some(s) = screen.0.as_mut() else { return };
        match cmd.clone() {
            SlotsInput::CloseHelp => {
                if s.help.take().is_some() {
                    info!("slots: help message closed");
                }
            }
            _ if s.help.is_some() => {}
            SlotsInput::Line(l) => s.forced = Some(l),
            SlotsInput::Exit => {
                if s.phase == Phase::Betting {
                    info!("slots: closed (the player left)");
                    next.set(PlayState::Playing);
                }
            }
            _ if s.phase != Phase::Betting => {}
            SlotsInput::Payouts => {
                s.payouts = !s.payouts;
                info!("slots: payout list {}", if s.payouts { "shown" } else { "hidden" });
            }
            SlotsInput::Bet(b) => s.bet = s.limits.clamp(b, s.chips),
            SlotsInput::Increase | SlotsInput::Decrease | SlotsInput::Max => {
                let b = match cmd {
                    SlotsInput::Increase => s.limits.step(s.bet, true, s.chips),
                    SlotsInput::Decrease => s.limits.step(s.bet, false, s.chips),
                    _ => s.limits.top(s.chips),
                };
                if b != s.bet {
                    if b > s.bet {
                        play(sounds::INCREASE[s.increases % 3]);
                        s.increases += 1;
                    } else {
                        play(sounds::DECREASE);
                    }
                    s.bet = b;
                    info!("slots: bet {}", s.bet);
                }
            }
            SlotsInput::Spin => {
                let bet = s.bet;
                if bet == 0 || bet > s.chips {
                    s.status = Some("You do not have enough chips.".into());
                    continue;
                }
                if let Err(e) = sim.world.remove_item(FormId::PLAYER_REF, s.chip, bet as i32) {
                    warn!("slots: the bet of {bet} chips not taken: {e:?}");
                    continue;
                }
                s.chips = s.chips.saturating_sub(bet);
                let n = s.reel.len().max(1);
                let to = match s.forced.take() {
                    // The test aid's line: the first stop showing each symbol.
                    Some(l) => std::array::from_fn(|i| rules::stop_of(&s.reel, l[i]).unwrap_or(0)),
                    None => rules::spin(&mut s.rng, n),
                };
                let from = s.stops;
                s.stops = to;
                let names = s.line().map(|x| rules::NAMES[x as usize]);
                s.payouts = false;
                s.status = None;
                s.phase = Phase::Spinning { since: now, from, to };
                stats.spins += 1;
                info!("slots: spin: bet {bet}, {} chips left; the reels will stop on {} {} {}", s.chips, names[0], names[1], names[2]);
                play(sounds::PULL);
            }
        }
    }
}

/// The spin's end: the line paid, the earnings counted; the result shown, then the next
/// spin (or, banned, the menu closes).
#[allow(clippy::too_many_arguments)]
fn step(time: AidTime, mut screen: ResMut<SlotsScreen>, mut next: ResMut<NextState<PlayState>>, mut sim: ResMut<SimState>, script: Option<ResMut<ScriptState>>, mut stats: ResMut<SlotsStats>, mut sounds: MessageWriter<PlaySound>) {
    let Some(s) = screen.0.as_mut() else { return };
    let now = time.elapsed_secs();
    match s.phase {
        Phase::Spinning { since, .. } if now - since >= REEL_STOPS[REELS - 1] => {
            let line = s.line();
            let multiple = rules::multiple(line);
            let back = s.bet * multiple;
            let net = back as i64 - s.bet as i64;
            if back > 0 {
                if let Err(e) = sim.world.add_item(FormId::PLAYER_REF, s.chip, back as i32) {
                    warn!("slots: winnings of {back} chips not given: {e:?}");
                }
                stats.wins += 1;
            }
            sounds.write(PlaySound { sound: win_sound(multiple), at: None });
            s.chips = carried(&sim, s.chip);
            if net >= 0 {
                stats.won += net as u64;
            } else {
                stats.lost += (-net) as u64;
            }
            if let Some(rt) = script.and_then(|x| x.into_inner().rt.as_mut()) {
                s.earnings = casino::add_earnings(&mut rt.game, s.casino, net, s.max_winnings);
                rt.game.count_stat(fnv_game::misc_stats::SLOTS_GAMES_PLAYED, 1.0);
                if casino::winnings_level(s.earnings, s.max_winnings) >= 4 {
                    s.banned = true;
                    stats.bans += 1;
                }
            }
            let names = line.map(|x| rules::NAMES[x as usize]);
            let won = match rules::win_name(line) {
                Some(w) => format!("{w} pays {multiple} x {} = {back} chips", s.bet),
                None => "nothing".to_string(),
            };
            info!(
                "slots: the reels stop on {} {} {}: {won}, net {net}; earnings {} of {} (level {}){}",
                names[0],
                names[1],
                names[2],
                s.earnings,
                s.max_winnings,
                casino::winnings_level(s.earnings, s.max_winnings),
                if s.banned { "; banned" } else { "" }
            );
            // [hypothesis: the original's words are not in the plugins; these are NVForever's]
            let result = match rules::win_name(line) {
                Some(w) => format!("{w}: you win {back} chips."),
                None => format!("You lose {} chips.", s.bet),
            };
            s.status = Some(if s.banned { format!("{result} You have been banned from {}.", s.casino_name) } else { result });
            s.phase = Phase::Paid { until: now + RESULT_SECONDS };
        }
        Phase::Paid { until } if now >= until => {
            if s.banned {
                info!("slots: closed (banned at {})", s.casino_name);
                next.set(PlayState::Playing);
                return;
            }
            s.phase = Phase::Betting;
            s.bet = s.limits.clamp(s.bet, s.chips);
            if s.chips == 0 {
                info!("slots: closed (no chips left)");
                next.set(PlayState::Playing);
            }
        }
        _ => {}
    }
}

fn close(mut screen: ResMut<SlotsScreen>, script: Option<ResMut<ScriptState>>) {
    if screen.0.take().is_none() {
        return;
    }
    if let Some(rt) = script.and_then(|s| s.into_inner().rt.as_mut()) {
        if rt.game.menu == Some(MENU_SLOTS) {
            rt.game.close_menu();
        }
    }
}

fn report(mut exit: MessageReader<AppExit>, stats: Res<SlotsStats>) {
    if exit.read().next().is_none() || stats.asked == 0 {
        return;
    }
    let s = &*stats;
    info!("slots summary: {} asked, {} opened ({} refused); {} spins, {} won, {} chips won, {} lost; {} bans", s.asked, s.opened, s.refused, s.spins, s.wins, s.won, s.lost, s.bans);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_aid_reads_its_steps() {
        let s = parse_aid("1:chips:200,2:open,3:bet:5,4:reels:6-6-6,5:spin,6:max,7:payout,8:exit,9:open:00140521,10:chips:9:140521").unwrap();
        assert_eq!(s[0], (1.0, AidStep::Chips(200, THE_TOPS)));
        assert_eq!(s[1], (2.0, AidStep::Open(THE_TOPS)));
        assert_eq!(s[2], (3.0, AidStep::Input(SlotsInput::Bet(5))));
        assert_eq!(s[3], (4.0, AidStep::Input(SlotsInput::Line([5, 5, 5]))));
        assert_eq!(s[5], (6.0, AidStep::Input(SlotsInput::Max)));
        assert_eq!(s[8], (9.0, AidStep::Open(0x0014_0521)));
        assert_eq!(s[9], (10.0, AidStep::Chips(9, 0x0014_0521)));
        assert!(parse_aid("1:reels:8-1-1").is_err());
        assert!(parse_aid("1:reels:1-1").is_err());
        assert!(parse_aid("1:pull").is_err());
    }

    #[test]
    fn win_sounds_by_size() {
        assert_eq!(win_sound(0), sounds::LOSE);
        assert_eq!(win_sound(2), sounds::WIN_SMALL);
        assert_eq!(win_sound(10), sounds::WIN_MED);
        assert_eq!(win_sound(160), sounds::WIN_JACKPOT);
    }
}
