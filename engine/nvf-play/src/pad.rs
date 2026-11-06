//! The controller (docs/design/controller.md): its buttons and stick directions, their
//! names, the game's default controller buttons, and the menu keys a controller stands
//! in for.
//!
//! **Which device works the controls** is the player's choice, `Input Device` in
//! `[Controls]` of `NVForever.ini` and the first row of the settings page's Controls tab
//! (`crate::bindings::InputDevice`) [design; owner 2026-10-16: keyboard and mouse, or a
//! controller, "depending on a setting under controls"]. With Controller chosen the
//! controls answer the controller only (`crate::bindings::ControlInput`), the left stick
//! moves, the right stick looks; Esc and the console's key still work.
//!
//! **The game's controller buttons** are the fourth byte `JJ` of each `[Controls]` value
//! of the owner's `FalloutPrefs.ini` (`crate::bindings::GAME_CONTROLS`) [observed
//! 2026-10-09; the byte's meaning is hypothesis, [`game_pad`]].
//!
//! **Menus** read keys, so while a controller works the controls its buttons stand in
//! for them ([`menu_keys`]): Start is Esc everywhere; in a menu (not playing) the D-pad
//! is the arrow keys, A Enter, B Esc, LB and RB, LT and RT Page Up and Page Down
//! [design]. On top of that each menu's own buttons press the key NVForever gives the
//! same button there ([`menu_buttons`]), as the original's menu files assign them
//! (`_xbox_button` on each button tile, e.g. `IM_DropButton` `&xbuttonx;`) [documented:
//! the vanilla menu files, read with `fnvtool ui tree`]; a menu's own button wins over
//! the shared ones (on the ITEMS page LB is Mod, not Page Up).

use bevy::prelude::*;

/// A controller input a control can be bound to: a button, or the left stick pushed one
/// way (the movement controls).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PadInput {
    Button(GamepadButton),
    Stick(StickDir),
}

/// A way the left stick is pushed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StickDir {
    Up,
    Down,
    Left,
    Right,
}

impl StickDir {
    pub const ALL: [StickDir; 4] = [StickDir::Up, StickDir::Down, StickDir::Left, StickDir::Right];

    fn index(self) -> usize {
        self as usize
    }

    /// How far a stick at `v` is pushed this way (0 to 1).
    pub fn amount(self, v: Vec2) -> f32 {
        match self {
            StickDir::Up => v.y,
            StickDir::Down => -v.y,
            StickDir::Left => -v.x,
            StickDir::Right => v.x,
        }
        .max(0.0)
    }
}

/// A stick pushed this far one way counts as that way's button pressed [design].
pub const STICK_PRESS: f32 = 0.5;
/// Below this push of the left stick the player walks, beyond it runs (with Always Run
/// on) [design].
pub const STICK_WALK_BELOW: f32 = 0.6;
/// A push of a stick under this counts as none [design; Bevy's own dead zone comes
/// first].
pub const STICK_DEAD: f32 = 0.12;

/// The game's controller code `JJ` of a `[Controls]` value as a controller input
/// [hypothesis]. The fit: the original's console layout puts Activate on A, the Pip-Boy
/// on B, Ready/Reload on X, Jump on Y, Aim on LT, Attack on RT, V.A.T.S. on RB, Wait on
/// Back and Sneak on a press of the left stick [documented: published Xbox 360 control
/// lists for the game, e.g. magicgameworld.com "Fallout: New Vegas Controls"], and the
/// owner's values give those controls `0A`, `0B`, `0C`, `0D`, `10`, `11`, `0E`, `07`,
/// `08` [observed]. A to Y follow the order of XInput's button bits, as Back then the
/// left then the right stick's press do [documented: Microsoft `XINPUT_GAMEPAD`
/// `wButtons`], so `09` (Grab) is taken for the right stick's press and `0F` (Toggle POV)
/// for the one shoulder left, LB. `13`, `14`, `16`, `17` (Forward, Back, Slide Right,
/// Slide Left) are taken for the left stick's directions. `01` (Ammo Swap) is not
/// placed; `FF` is none.
pub fn game_pad(code: u8) -> Option<PadInput> {
    use GamepadButton::*;
    Some(match code {
        0x07 => PadInput::Button(Select),
        0x08 => PadInput::Button(LeftThumb),
        0x09 => PadInput::Button(RightThumb),
        0x0A => PadInput::Button(South),
        0x0B => PadInput::Button(East),
        0x0C => PadInput::Button(West),
        0x0D => PadInput::Button(North),
        0x0E => PadInput::Button(RightTrigger),
        0x0F => PadInput::Button(LeftTrigger),
        0x10 => PadInput::Button(LeftTrigger2),
        0x11 => PadInput::Button(RightTrigger2),
        0x13 => PadInput::Stick(StickDir::Up),
        0x14 => PadInput::Stick(StickDir::Down),
        0x16 => PadInput::Stick(StickDir::Right),
        0x17 => PadInput::Stick(StickDir::Left),
        _ => return None,
    })
}

/// The inputs' names in the INI and on screen ("A)" under the crosshair) [design: an
/// Xbox controller's, as the original's button pictures are (`general_button_a.dds` …
/// `general_button_rs.dds`) [observed: `button_xbox_hint.xml`]].
pub const PAD_NAMES: &[(PadInput, &str)] = {
    use GamepadButton::*;
    use PadInput::{Button as B, Stick as S};
    &[
        (B(South), "A"), (B(East), "B"), (B(West), "X"), (B(North), "Y"),
        (B(LeftTrigger), "LB"), (B(RightTrigger), "RB"), (B(LeftTrigger2), "LT"), (B(RightTrigger2), "RT"),
        (B(LeftThumb), "LS"), (B(RightThumb), "RS"), (B(Select), "Back"), (B(Start), "Start"),
        (B(DPadUp), "D-Pad Up"), (B(DPadDown), "D-Pad Down"), (B(DPadLeft), "D-Pad Left"), (B(DPadRight), "D-Pad Right"),
        (S(StickDir::Up), "Left Stick Up"), (S(StickDir::Down), "Left Stick Down"),
        (S(StickDir::Left), "Left Stick Left"), (S(StickDir::Right), "Left Stick Right"),
    ]
};

/// Start opens and closes the pause menu: no control may take it.
pub const RESERVED_PAD: [PadInput; 1] = [PadInput::Button(GamepadButton::Start)];

pub fn pad_name(p: PadInput) -> &'static str {
    PAD_NAMES.iter().find(|(x, _)| *x == p).map_or("?", |(_, n)| n)
}

/// The input of a name (without case); `None` for none or an unknown name.
pub fn parse_pad(name: &str) -> Option<PadInput> {
    PAD_NAMES.iter().find(|(_, n)| n.eq_ignore_ascii_case(name.trim())).map(|(p, _)| *p)
}

/// The left stick's directions as buttons, this frame and the last (for a just-pressed),
/// any controller's.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct StickButtons {
    pub now: [bool; 4],
    pub before: [bool; 4],
}

impl StickButtons {
    pub fn pressed(&self, d: StickDir) -> bool {
        self.now[d.index()]
    }

    pub fn just_pressed(&self, d: StickDir) -> bool {
        self.now[d.index()] && !self.before[d.index()]
    }

    pub fn just_released(&self, d: StickDir) -> bool {
        !self.now[d.index()] && self.before[d.index()]
    }
}

/// The furthest-pushed stick of the connected controllers (`left` or right), dead zone
/// taken off.
pub fn stick(pads: &Query<&Gamepad>, left: bool) -> Vec2 {
    let v = pads.iter().map(|g| if left { g.left_stick() } else { g.right_stick() }).max_by(|a, b| a.length_squared().total_cmp(&b.length_squared())).unwrap_or(Vec2::ZERO);
    if v.length() < STICK_DEAD {
        Vec2::ZERO
    } else {
        v.clamp_length_max(1.0)
    }
}

fn update_stick_buttons(pads: Query<&Gamepad>, mut s: ResMut<StickButtons>) {
    let v = stick(&pads, true);
    s.before = s.now;
    for d in StickDir::ALL {
        s.now[d.index()] = d.amount(v) >= STICK_PRESS;
    }
}

/// The menu keys a controller's buttons hold down now (module notes).
#[derive(Resource, Debug, Default)]
pub struct MenuKeys {
    held: Vec<KeyCode>,
}

/// Start: Esc, always.
const ALWAYS: [(GamepadButton, KeyCode); 1] = [(GamepadButton::Start, KeyCode::Escape)];
/// In a menu.
const IN_MENUS: [(GamepadButton, KeyCode); 10] = [
    (GamepadButton::DPadUp, KeyCode::ArrowUp),
    (GamepadButton::DPadDown, KeyCode::ArrowDown),
    (GamepadButton::DPadLeft, KeyCode::ArrowLeft),
    (GamepadButton::DPadRight, KeyCode::ArrowRight),
    (GamepadButton::South, KeyCode::Enter),
    (GamepadButton::East, KeyCode::Escape),
    (GamepadButton::LeftTrigger, KeyCode::PageUp),
    (GamepadButton::RightTrigger, KeyCode::PageDown),
    (GamepadButton::LeftTrigger2, KeyCode::PageUp),
    (GamepadButton::RightTrigger2, KeyCode::PageDown),
];

/// The menu open on top ([`fnv_game::Game::top_menu`]'s MenuMode code) and the Pip-Boy's
/// tab, for [`menu_buttons`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MenuAt {
    pub code: Option<u16>,
    pub tab: usize,
}

/// A menu's own controller buttons: the key each stands in for there. The button each
/// menu file gives a control [documented: `_xbox_button` of the button tiles, below as
/// "file"], pressing the key NVForever's hand-built menu answers for that control.
pub fn menu_buttons(at: MenuAt) -> &'static [(GamepadButton, KeyCode)] {
    use fnv_game::game::menus;
    use GamepadButton::{East, LeftTrigger, LeftTrigger2, North, RightTrigger2, South, West};
    // The Pip-Boy: LT and RT step through its pages, STATS, ITEMS, DATA and round again,
    // through their keys F1-F3 (`crate::pipboy`); the D-pad's left and right step the tabs
    // as the arrow keys do [design: owner 2026-11-02; the original's page buttons are not in
    // its menu files].
    use KeyCode::*;
    let Some(code) = at.code else { return &[] };
    match code {
        // container_menu.xml: Take All X.
        crate::script::MENU_CONTAINER => &[(West, KeyR)],
        // barter_menu.xml: Accept X (B exits, as Esc).
        menus::BARTER => &[(West, KeyA)],
        // inventory_menu.xml: Repair Y, Mod LB (Drop X: `crate::pipboy::items`, which has
        // no key for it; Hotkey RB: the hotkeys are held digits, not built for the pad).
        menus::INVENTORY => &[(North, KeyR), (LeftTrigger, KeyX), (LeftTrigger2, F1), (RightTrigger2, F3)],
        // stats_menu.xml: Stimpak A, Doctor's Bag Y, Rad-X X on the status tab; the
        // General/Reputation switch X on the General tab.
        menus::STATS if at.tab == 0 => &[(South, KeyS), (North, KeyE), (West, KeyX), (LeftTrigger2, F3), (RightTrigger2, F2)],
        menus::STATS if at.tab == 4 => &[(West, KeyR), (LeftTrigger2, F3), (RightTrigger2, F2)],
        menus::STATS => &[(LeftTrigger2, F3), (RightTrigger2, F2)],
        // map_menu.xml: the Challenges toggle Y (R on the Notes tab).
        menus::MAP => &[(North, KeyR), (LeftTrigger2, F2), (RightTrigger2, F1)],
        // recipe_menu.xml: Accept X.
        fnv_game::crafting::RECIPE_MENU => &[(West, KeyA)],
        // repair_services_menu.xml: Repair All X.
        fnv_game::repair::REPAIR_SERVICES_MENU => &[(West, KeyA)],
        // levelup_menu.xml, trait_menu.xml: Reset Y, Continue X, Back B.
        menus::LEVEL_UP | menus::TRAITS => &[(North, KeyR), (West, KeyA), (East, KeyE)],
        // char_gen_menu.xml (the tag skills): Reset Y, Done X.
        menus::TAG_SKILLS => &[(North, KeyR), (West, KeyA)],
        // lockpick_menu.xml: Force Lock X.
        menus::LOCKPICK => &[(West, KeyF)],
        // black_jack_menu.xml: Hit A, Double/Deal Y, Split/Raise RT, Switch/Lower LT,
        // Surrender/Max Bet X, Stay/Exit B.
        fnv_game::blackjack::MENU_CODE => &[(South, KeyF), (North, KeyW), (RightTrigger2, KeyE), (LeftTrigger2, KeyQ), (West, KeyS), (East, KeyR)],
        // roulette_menu.xml: Place A, Remove Y, Finish X, Lower LT, Raise RT, Exit B.
        fnv_game::roulette::MENU_CODE => &[(South, KeyW), (North, KeyF), (West, KeyS), (LeftTrigger2, KeyQ), (RightTrigger2, KeyE), (East, KeyR)],
        // slot_machine_menu.xml: Spin A, Raise RT, Lower LT, Payouts Y, Max Bet X, Exit B.
        fnv_game::slots::MENU_CODE => &[(South, KeyW), (RightTrigger2, KeyE), (LeftTrigger2, KeyQ), (North, KeyF), (West, KeyS), (East, KeyR)],
        // tutorial_menu.xml: Next RT, Previous LT.
        menus::TUTORIAL => &[(RightTrigger2, KeyA), (LeftTrigger2, KeyS)],
        // A conversation (1009): B (East) leaves as Tab does (the Goodbye choice, or the
        // topic list's end). While the actor speaks Tab does nothing, so B does nothing
        // there too, matching the original [observed: play, issue #1657].
        menus::DIALOGUE => &[(East, Tab)],
        _ => &[],
    }
}

/// Press and release the keys the controller's buttons stand for, after the input
/// systems have read the keyboard (so a menu sees a just-pressed key this frame). A key
/// goes down only as its button does (a button still held from before, as after binding
/// it on the Controls tab, presses nothing); none while the Controls tab waits for a new
/// button, which takes the buttons themselves.
#[allow(clippy::type_complexity)]
pub(crate) fn menu_keys(
    pads: Query<&Gamepad>,
    bindings: Option<Res<crate::bindings::Bindings>>,
    (state, paused, page): (Option<Res<State<crate::PlayState>>>, Option<Res<crate::pause::Paused>>, Option<Res<crate::pause::Page>>),
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mk: ResMut<MenuKeys>,
    (script, pip): (Option<Res<crate::script::ScriptState>>, Option<Res<crate::pipboy::PipBoy>>),
) {
    let waiting = page.is_some_and(|p| matches!(*p, crate::pause::Page::Controls { listening: true, .. }));
    let on = bindings.is_some_and(|b| b.device == crate::bindings::InputDevice::Controller) && !waiting;
    let in_menu = state.is_some_and(|s| *s.get() != crate::PlayState::Playing) || paused.is_some_and(|p| p.0);
    let mut want: Vec<KeyCode> = Vec::new();
    if on {
        let at = MenuAt { code: script.as_ref().and_then(|s| s.rt.as_ref()).and_then(|rt| rt.game.top_menu()), tab: pip.map_or(0, |p| p.tab()) };
        let own = if in_menu { menu_buttons(at) } else { &[] };
        let shared = ALWAYS.iter().chain(if in_menu { IN_MENUS.iter() } else { [].iter() }).filter(|(b, _)| !own.iter().any(|(o, _)| o == b));
        for (b, k) in own.iter().chain(shared) {
            let down = if mk.held.contains(k) { pads.iter().any(|g| g.pressed(*b)) } else { pads.iter().any(|g| g.just_pressed(*b)) };
            if down && !want.contains(k) {
                want.push(*k);
            }
        }
    }
    for k in mk.held.clone() {
        if !want.contains(&k) {
            keys.release(k);
        }
    }
    for k in &want {
        if !mk.held.contains(k) {
            keys.press(*k);
        }
    }
    mk.held = want;
}

/// The key a controller button stands in for in a menu ([`menu_keys`]'s tables), when the
/// controller works the controls: what the menus' own key handlers then see.
pub(crate) fn menu_key(button: GamepadButton) -> Option<KeyCode> {
    ALWAYS.iter().chain(IN_MENUS.iter()).find(|(b, _)| *b == button).map(|(_, k)| *k)
}

/// [`menu_key`] in the menu `at`: what its own buttons stand for there first, as
/// [`menu_keys`] has them ([`menu_buttons`]; the tutorial menu's triggers are its A and S, not
/// Page Up and Page Down; the container menu's X is its R).
pub(crate) fn menu_key_at(button: GamepadButton, at: MenuAt) -> Option<KeyCode> {
    menu_buttons(at).iter().find(|(b, _)| *b == button).map(|(_, k)| *k).or_else(|| menu_key(button))
}

/// Whether `button` went down this frame on a controller that works the controls: for a
/// menu's own controller buttons beyond the keys [`menu_keys`] stands in for (the ITEMS
/// page's X drop, `crate::pipboy::items`).
pub(crate) fn menu_button(pads: &Query<&Gamepad>, bindings: Option<&crate::bindings::Bindings>, button: GamepadButton) -> bool {
    bindings.is_some_and(|b| b.device == crate::bindings::InputDevice::Controller) && pads.iter().any(|g| g.just_pressed(button))
}

/// A controller's arrival and departure, in the log.
fn log_pads(added: Query<(Entity, Option<&Name>), Added<Gamepad>>, mut gone: RemovedComponents<Gamepad>) {
    for (e, name) in &added {
        info!("controller: {} connected ({e})", name.map_or("a controller", |n| n.as_str()));
    }
    for e in gone.read() {
        info!("controller: {e} disconnected");
    }
}

pub struct PadPlugin;

impl Plugin for PadPlugin {
    fn build(&self, app: &mut App) {
        use bevy::input::InputSystems;
        app.init_resource::<StickButtons>()
            .init_resource::<MenuKeys>()
            .add_systems(PreUpdate, (update_stick_buttons, menu_keys).after(InputSystems))
            .add_systems(Update, log_pads);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menus_own_buttons_follow_the_files() {
        use fnv_game::game::menus;
        use GamepadButton::*;
        let at = |code, tab| MenuAt { code: Some(code), tab };
        // The ITEMS page: Repair Y, Mod LB (the keyboard's R and X); X is the drop button.
        assert!(menu_buttons(at(menus::INVENTORY, 0)).starts_with(&[(North, KeyCode::KeyR), (LeftTrigger, KeyCode::KeyX)]));
        assert!(menu_buttons(at(menus::INVENTORY, 0)).iter().all(|(b, _)| *b != West));
        assert!(menu_buttons(at(crate::script::MENU_CONTAINER, 0)).contains(&(West, KeyCode::KeyR)));
        assert!(menu_buttons(at(menus::BARTER, 0)).contains(&(West, KeyCode::KeyA)));
        assert!(menu_buttons(at(menus::STATS, 4)).contains(&(West, KeyCode::KeyR)));
        // The triggers step the Pip-Boy's pages, round from DATA to STATS.
        for (code, lt, rt) in [(menus::STATS, KeyCode::F3, KeyCode::F2), (menus::INVENTORY, KeyCode::F1, KeyCode::F3), (menus::MAP, KeyCode::F2, KeyCode::F1)] {
            for tab in 0..5 {
                let t = menu_buttons(at(code, tab));
                assert!(t.contains(&(LeftTrigger2, lt)) && t.contains(&(RightTrigger2, rt)), "menu {code} tab {tab}");
            }
        }
        assert!(menu_buttons(MenuAt::default()).is_empty());
        // No menu gives one button two keys.
        for code in 1000..1200 {
            for tab in 0..5 {
                let t = menu_buttons(at(code, tab));
                for (i, (b, _)) in t.iter().enumerate() {
                    assert!(t[i + 1..].iter().all(|(o, _)| o != b), "menu {code} tab {tab}: {b:?} twice");
                }
            }
        }
    }

    #[test]
    fn dialogue_owns_b_as_tab_not_escape() {
        use fnv_game::game::menus;
        use GamepadButton::East;
        let at = MenuAt { code: Some(menus::DIALOGUE), tab: 0 };
        // B (East) is the conversation's own leave key (Tab/Goodbye), not the shared Esc
        // that opens the pause menu (#1657).
        assert!(menu_buttons(at).contains(&(East, KeyCode::Tab)));
        assert_eq!(menu_key_at(East, at), Some(KeyCode::Tab));
        // Outside a conversation East is still the shared Esc.
        assert_eq!(menu_key(East), Some(KeyCode::Escape));
    }

    #[test]
    fn the_games_codes_give_the_console_layout() {
        use GamepadButton::*;
        assert_eq!(game_pad(0x0A), Some(PadInput::Button(South)));
        assert_eq!(game_pad(0x11), Some(PadInput::Button(RightTrigger2)));
        assert_eq!(game_pad(0x0E), Some(PadInput::Button(RightTrigger)));
        assert_eq!(game_pad(0x13), Some(PadInput::Stick(StickDir::Up)));
        assert_eq!(game_pad(0xFF), None);
        assert_eq!(game_pad(0x01), None);
    }

    #[test]
    fn names_read_back() {
        for (p, n) in PAD_NAMES {
            assert_eq!(parse_pad(n), Some(*p));
            assert_eq!(pad_name(*p), *n);
        }
        assert_eq!(parse_pad(" left stick up "), Some(PadInput::Stick(StickDir::Up)));
        assert_eq!(parse_pad("Z"), None);
    }

    #[test]
    fn a_stick_pushed_one_way() {
        let v = Vec2::new(0.3, -0.8);
        assert_eq!(StickDir::Down.amount(v), 0.8);
        assert_eq!(StickDir::Up.amount(v), 0.0);
        assert!((StickDir::Right.amount(v) - 0.3).abs() < 1e-6);
    }
}
