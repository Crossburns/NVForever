//! The tile tree beside the drawn menus [design: ui-tile-tree.md §6.2 step 1].

pub mod draw;
pub mod input;
pub mod layout;
pub mod menus;
pub mod model;
pub mod mods;
pub mod notices;
pub mod vanilla;
pub mod wait_menu;

use crate::{script::ScriptState, PlaySet, PlayState};
use bevy::{ecs::system::SystemParam, prelude::*, window::PrimaryWindow};
use fnv_game::ui_loading::{self, UiFiles};
use fnv_ui::Value;
use nvf_assets::GameData;

#[derive(SystemParam)]
pub(crate) struct MenuFiles<'w, 's> {
    data: Option<Res<'w, GameData>>,
    windows: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
    paused: Option<Res<'w, crate::pause::Paused>>,
    main_menu: Option<Res<'w, crate::main_menu::MainMenu>>,
    menus: Option<Res<'w, vanilla::TileMenus>>,
}

impl MenuFiles<'_, '_> {
    pub(crate) fn load(&self, game: &mut fnv_game::Game<'_>, world: &fnv_sim::World<'_>) {
        let Some(data) = self.data.as_ref() else { return };
        let files = UiFiles::new(data.vfs.clone(), world);
        let paths = files.menu_paths();
        let report = ui_loading::load(game, Box::new(files), paths);
        self.write(game);
        game.ui_tree.update();
        for (file, messages) in &report.warnings {
            warn!("ui: {file}: {}", messages.join("; "));
        }
        info!("ui: {} menus loaded ({} tiles), {} warnings", report.menus, report.tiles, report.warning_count());
        // T10 (menu-xml.md §2.4): names with no GMST record read NVForever's own labels.
        info!("ui: game strings: {} read, {} without text", report.game_strings, report.game_strings_unresolved);
    }

    pub(crate) fn write(&self, game: &mut fnv_game::Game<'_>) {
        if game.ui_files.is_none() {
            return;
        }
        if let Ok(window) = self.windows.single() {
            // T15a: interface units only while a T15 menu is drawn from its file.
            let units = self.menus.as_ref().is_some_and(|m| interface_screen(**m));
            write_screen(&mut game.ui_tree, window.width(), window.height(), units);
        }
        let report = ui_loading::sync(game, self.paused.as_ref().is_some_and(|p| p.0), self.main_menu.as_ref().is_some_and(|m| m.open));
        for line in report.changes {
            info!("ui: {line}");
        }
        for line in report.warnings {
            warn!("ui: {line}");
        }
    }
}

/// Whether `screen()` is written in interface units ([`write_screen`]): only while a T15
/// menu (`MessageMenu`, `QuantityMenu`, `SleepWaitMenu`) or the start menu's Mods page
/// (T11, `mods`) is switched on, whose files centre their boxes by `screen()` (MCM's `x`
/// and `y`, menu-xml.md §10). The default game and the tile HUD alone keep the window's
/// pixels [design]. The original reports interface units [observed: hud_menus capture
/// 2026-10-31, three window sizes]; applying them unconditionally is a follow-up.
pub fn interface_screen(menus: vanilla::TileMenus) -> bool {
    menus.message || menus.quantity || menus.sleep_wait || menus.start
}

/// Write `screen()`'s traits for a window `window_width` x `window_height` (logical
/// pixels): with `interface_units` ([`interface_screen`]) the view in interface units
/// ([`screen_size`]) and `resolutionconverter` ([`resolution_converter`]); otherwise the
/// window's size, as fnvtool's `--screen`, and no converter, as before T15a [design:
/// menu-xml.md §4.2, `screen()` row]. `cropx` and `cropy` stay unset until their units are
/// known [design]. The HUD's places are worked out from the shape alone
/// (`fnv_game::ui_hud::write`), so they are the same either way.
pub fn write_screen(ui: &mut fnv_ui::Ui, window_width: f32, window_height: f32, interface_units: bool) {
    let id = ui.screen();
    if interface_units {
        let (width, height) = screen_size(window_width, window_height);
        ui.set_trait(id, "width", Value::Num(width));
        ui.set_trait(id, "height", Value::Num(height));
        ui.set_trait(id, "resolutionconverter", Value::Num(resolution_converter(window_height)));
    } else {
        ui.set_trait(id, "width", Value::Num(f64::from(window_width)));
        ui.set_trait(id, "height", Value::Num(f64::from(window_height)));
    }
}

/// `screen()`'s width and height for a window in interface units: 960 high and
/// 960 × window width/height wide [observed: hud_menus capture 2026-10-31, three window sizes].
/// The windows were 1280 x 720, 1920 x 1080 and 1280 x 1024 (also hud.md §14's
/// right-hand pieces at W - 386). Written only while a T15 menu is switched on ([`interface_screen`]);
/// applying these observed units unconditionally is a follow-up [design].
pub fn screen_size(window_width: f32, window_height: f32) -> (f64, f64) {
    let (w, h) = (f64::from(window_width), f64::from(window_height));
    if w > 0.0 && h > 0.0 {
        (960.0 * w / h, 960.0)
    } else {
        (0.0, 0.0)
    }
}

/// `screen()`'s `resolutionconverter` for a window `window_height` pixels high:
/// 960 / height [observed: hud_menus capture 2026-10-31, three window sizes]. The windows
/// were 1280 x 720, 1920 x 1080 and 1280 x 1024. Thus `globals()`'s PC `_line_thickness` (2 × the
/// converter) is 2 pixels thick [documented: globals.xml]. Written only with interface
/// units ([`write_screen`]); the unconditional switch is a follow-up [design].
pub fn resolution_converter(window_height: f32) -> f64 {
    if window_height > 0.0 {
        960.0 / f64::from(window_height)
    } else {
        0.0
    }
}

/// Read the same inputs as the current HUD, before scripts [design]. Timed pieces
/// use the current target/queue/meter state; their clocks remain with the drawing.
#[derive(SystemParam)]
struct HudTiles<'w> {
    state: Res<'w, State<PlayState>>,
    sim: Res<'w, crate::sim::SimState>,
    control: Option<Res<'w, crate::control::PlayerControl>>,
    locks: Option<Res<'w, crate::control::ControlLocks>>,
    driven: Option<Res<'w, crate::player::ViewDriven>>,
    wheel: Option<Res<'w, crate::weapon_wheel::WeaponWheel>>,
    creator: Option<Res<'w, crate::creator::CreatorMenu>>,
    zoom: Option<Res<'w, crate::aim_zoom::AimZoom>>,
    sights: Option<Res<'w, crate::aim_zoom::TrueSights>>,
    scope: Option<Res<'w, crate::aim_zoom::Scope>>,
    combat: Option<Res<'w, crate::combat::Combat>>,
    target: Option<Res<'w, crate::activate::Target>>,
    bindings: Option<Res<'w, crate::bindings::Bindings>>,
    enemy: Option<Res<'w, crate::hud_target::HudTarget>>,
    aim_hostile: Option<Res<'w, crate::hud_target::AimHostile>>,
    limbs: Option<Res<'w, crate::hud_limbs::Indicator>>,
    player_limbs: Option<Res<'w, crate::combat::PlayerLimbs>>,
    breath: Option<Res<'w, crate::drowning::PlayerBreath>>,
    xp: Option<Res<'w, crate::xp_meter::Meter>>,
    corner: Option<Res<'w, crate::corner_messages::CornerMessages>>,
    quests: Option<Res<'w, crate::quest_messages::QuestMessages>>,
    hud: Option<Res<'w, crate::script_hud::ScriptHud>>,
    player: Option<Res<'w, crate::player::Player>>,
    panel: Option<Res<'w, crate::message_menu::MessagePanel>>,
    loading: Option<Res<'w, crate::loading::Loading>>,
    bars: Option<Res<'w, crate::hud::HudView>>,
    compass: Option<Res<'w, crate::compass::CompassView>>,
}

impl HudTiles<'_> {
    fn shown(&self, game: &fnv_game::Game<'_>, files: &MenuFiles<'_, '_>) -> fnv_game::ui_hud::Shown {
        let playing = *self.state.get() == PlayState::Playing;
        let root = playing
            && !game.in_menu()
            && !files.paused.as_ref().is_some_and(|p| p.0)
            && !files.main_menu.as_ref().is_some_and(|m| m.open)
            && !self.loading.as_ref().is_some_and(|l| l.active());
        // A playing kill camera hides all of it, as an idle holding the camera does
        // (`crate::kill_cam`) [owner's playtest request #1034].
        let driven = self.driven.as_ref().is_some_and(|d| d.0) || self.bars.as_ref().is_some_and(|b| b.kill_cam);
        let wheel = self.wheel.as_ref().is_some_and(|w| w.is_open());
        let creating = self.creator.as_ref().is_some_and(|c| c.is_open());
        let locks = self.locks.as_deref().copied().unwrap_or_default();
        // hud::gather and hud::{draw,texts}: the opening's movement lock, portrait,
        // held camera and weapon wheel hide the corner HUD and its meters [design].
        let corners = playing && !locks.movement && !self.control.as_ref().is_some_and(|c| c.portrait.is_some()) && !driven && !wheel;
        let weapon = self.combat.as_ref().map(|c| c.hud_weapon(&self.sim)).unwrap_or_default();
        let enemy = self.enemy.as_ref().and_then(|e| e.0.as_ref()).filter(|e| e.health < 1.0);
        // reticle::draw: the prompt also goes under true sights/the scope, and when
        // the enemy bar names this target; only the crosshair obeys movement [design].
        let reticle = playing && !creating && !driven && !wheel && !self.sights.as_ref().is_some_and(|s| s.0) && !self.scope.as_ref().is_some_and(|s| s.on);
        let info = reticle && !locks.rollover && self.target.as_ref().and_then(|t| t.0.as_ref()).is_some_and(|t| !enemy.is_some_and(|e| crate::reticle::under_bar(t, e.form_id)));
        // corner_messages::draw and quest_messages::draw [design].
        let notices = !driven && *self.state.get() != PlayState::Dialogue && !self.panel.as_ref().is_some_and(|p| p.view.is_some());
        let quest = self.quests.as_ref().and_then(|q| q.showing());
        // Map discovery to Region_Location [hypothesis: hud.md §14; one queue here]. The
        // tile HUD writes it under both: Region_Location's copies hold its traits (not
        // drawn), QuestReminder's are drawn, where the owner's screenshot shows it
        // (`notices`, hud.md §14.11).
        let location = matches!(quest, Some(crate::script_events::QuestNotice::Discovered(_)));
        // hud_needs::watch/draw: H20/FOD/SLP in Hardcore, RAD and LMB in every mode
        // [design: hud.md §13; observed words: hud.md §14.2].
        // The rect shows if any word has a look other than Hidden [design].
        use fnv_game::aid::{DEHYDRATION, HUNGER, RAD_LEVEL, SLEEP_DEPRIVATION};
        let need = |code| game.player.need_stages.get(&code).is_some_and(|held| game.data.need_stages.get(&code).is_some_and(|stages| stages.iter().any(|s| s.stage == *held)));
        // needs::hardcore with the live world's config, as hud_needs::watch [design].
        let hardcore = game.player.hardcore.unwrap_or(self.sim.world.config().hardcore);
        // Only the six limbs count, as hud_needs::watch (the brain is excluded) [design].
        let needs = need(RAD_LEVEL)
            || (hardcore && [DEHYDRATION, HUNGER, SLEEP_DEPRIVATION].into_iter().any(need))
            || self.player_limbs.as_ref().is_some_and(|l| (25..=30).any(|code| l.crippled(code)));
        fnv_game::ui_hud::Shown {
            root,
            aiming: playing && self.zoom.as_ref().is_some_and(|z| z.aimed),
            corners,
            sneak: corners && self.control.as_ref().is_some_and(|c| c.sneak),
            reticle: reticle && !locks.movement,
            enemy: corners && enemy.is_some(),
            quests: notices && quest.is_some() && !location,
            messages: notices && self.corner.as_ref().is_some_and(|c| c.showing().is_some()),
            subtitles: !creating && self.hud.as_ref().is_some_and(|h| crate::script_hud::hud_subtitles_shown(h, game, &self.sim, self.player.as_deref())),
            info,
            location: notices && location,
            xp: corners && self.xp.as_ref().is_some_and(|m| m.show.is_some()), // xp_meter::draw [design].
            radiation: corners && game.radiation().rate > 0.0,                 // radiation::draw [design].
            breath: corners && self.breath.as_ref().is_some_and(|b| crate::hud_breath::shown(b)),
            limbs: corners && self.limbs.as_ref().is_some_and(|l| l.shown()), // hud_limbs::draw [design].
            needs: corners && needs,
            ammo_label: corners && weapon.ammo.is_some() && weapon.abbrev.as_ref().is_some_and(|s| !s.is_empty()),
            condition: corners && weapon.condition.is_some(),
        }
    }

    /// What the tile-drawn corners take beyond T4's writes (`vanilla::write_corners`): the
    /// hand-built HUD's label places as the copies' tops and edges, the right piece's moved
    /// from its `MARGIN_X` in to the tile piece's place on the view's edge [observed: hud.md
    /// §14, `ActionPoints` at W - 386; design: `hud::label_places`], and how far the
    /// condition row moves for the ammo abbreviation (its width in font 7).
    fn corner(&self, shown: fnv_game::ui_hud::Shown, fonts: &mut draw::FontKit) -> vanilla::Corner {
        let (labels, line) = fonts.hud_labels();
        let shift = crate::hud::MARGIN_X;
        let top = |(x, base): (f32, f32), shift: f32| (f64::from(x - shift), f64::from(base - line));
        let weapon = self.combat.as_ref().map(|c| c.hud_weapon(&self.sim)).unwrap_or_default();
        // The row's place follows the weapon even while the corners are hidden (a menu,
        // the console), as the pieces' places hold [design: hud.md §14.1].
        let abbrev = weapon.abbrev.clone().filter(|_| weapon.ammo.is_some()).unwrap_or_default();
        let condition_shift = vanilla::condition_shift(&abbrev, f64::from(fonts.hud_width(&abbrev)));
        let ammo_label = if shown.ammo_label { abbrev } else { String::new() };
        vanilla::Corner {
            hp: top(labels[0], 0.0),
            ap: top(labels[1], shift),
            cnd: top(labels[2], shift),
            ammo: top(labels[3], shift),
            right_bracket: vanilla::RIGHT_BRACKET_X,
            condition_shift,
            ammo_label,
            condition: shown.condition,
        }
    }

    /// What the tile-drawn messages, quest notices and subtitles show (`notices::write`):
    /// the hand-built queues' lines on screen with their strengths, and the subtitles the
    /// hand-built HUD draws, under the same showing rules as [`HudTiles::shown`]'s
    /// (`messages`, `quests`/`location`, `subtitles`) [design: their rules stay the
    /// hand-built HUD's].
    fn notices(&self, game: &fnv_game::Game<'_>, shown: fnv_game::ui_hud::Shown) -> notices::Notices {
        use crate::script_events::QuestNotice;
        let message = self.corner.as_ref().filter(|_| shown.messages).and_then(|c| {
            let note = c.showing()?;
            Some(notices::Message {
                text: note.text.clone(),
                icon: note.icon.clone().unwrap_or_else(|| crate::corner_messages::DEFAULT_ICON.into()),
                strength: f64::from(c.strength()),
            })
        });
        let quest = self.quests.as_ref().filter(|_| shown.quests || shown.location).and_then(|q| {
            let n = q.showing()?;
            let strength = f64::from(q.strength(0.0));
            Some(match n {
                QuestNotice::Objectives(list) => notices::Quest::Objectives { list: list.iter().map(|o| (crate::quest_messages::objective_text(o), o.completed)).collect(), strength },
                QuestNotice::Added(name) | QuestNotice::Completed(name) | QuestNotice::Failed(name) | QuestNotice::Discovered(name) => {
                    let discovered = matches!(n, QuestNotice::Discovered(_));
                    notices::Quest::Banner { label: crate::quest_messages::label(n).unwrap_or_default().into(), name: name.clone(), strength, discovered }
                }
            })
        });
        let subtitles = match &self.hud {
            Some(h) if shown.subtitles => crate::script_hud::hud_subtitle_texts(h, game, &self.sim, self.player.as_deref()),
            _ => Vec::new(),
        };
        notices::Notices { message, quest, subtitles }
    }

    fn texts(&self, game: &mut fnv_game::Game<'_>, shown: fnv_game::ui_hud::Shown) -> fnv_game::ui_hud::Texts {
        // Read the drawing's current values before scripts, as for Shown [design:
        // ui-tile-tree.md §4.1]; the strings' file tiles [observed: hud.md §14.2].
        let prompt = self.target.as_ref().and_then(|t| t.0.as_ref()).filter(|_| shown.info).map(|t| crate::reticle::Prompt::bound(t, &self.sim, self.bindings.as_deref()));
        // Reuse the latest drawing snapshot before scripts [design: ui-tile-tree.md §4.1].
        let weapon = self.combat.as_ref().map(|c| c.hud_weapon(&self.sim)).unwrap_or_default();
        // Hostility as `hud_target::aim` worked it out this frame, so both HUDs agree.
        let enemy = self.enemy.as_ref().and_then(|e| e.0.as_ref()).map(|e| fnv_game::ui_hud::Enemy { name: e.name.clone(), health: f64::from(e.health), hostile: e.hostile });
        use fnv_game::aid::{DEHYDRATION, HUNGER, SLEEP_DEPRIVATION, RAD_LEVEL};
        let need_stages = [DEHYDRATION, HUNGER, SLEEP_DEPRIVATION, RAD_LEVEL].map(|code| {
            game.player.need_stages.get(&code).and_then(|held| game.data.need_stages.get(&code)?.iter().position(|s| s.stage == *held)).map_or(0, |i| i + 1)
        });
        let frame = fnv_game::ui_hud::Frame {
            aim_hostile: self.aim_hostile.as_ref().is_some_and(|h| h.0),
            enemy,
            hardcore: game.player.hardcore.unwrap_or(self.sim.world.config().hardcore),
            need_stages,
            crippled: std::array::from_fn(|i| self.player_limbs.as_ref().is_some_and(|l| l.crippled(25 + i as u16))),
            animation: Default::default(),
            health: self.bars.as_ref().map_or(1.0, |v| f64::from(v.health)),
            action_points: self.bars.as_ref().map_or(1.0, |v| f64::from(v.action_points)),
            condition: weapon.condition.map(f64::from),
            ammo: crate::hud::ammo_text(weapon.ammo),
            radiation: fnv_game::functions::actor_value(game, fnv_sim::FormId::PLAYER_REF, fnv_game::aid::RAD_LEVEL).unwrap_or(0.0),
            prompt: prompt.as_ref().map(|p| fnv_game::ui_hud::Prompt {
                name: p.name.clone(),
                weight: p.figures.as_ref().map_or_else(|| "--".into(), |f| f[0].clone()),
                value: p.figures.as_ref().map_or_else(|| "--".into(), |f| f[1].clone()),
                empty: p.note.is_some(),
                item: p.figures.is_some(),
                lock_text: self.target.as_ref().and_then(|t| t.0.as_ref()).map_or_else(|| "Locked".into(), |t| {
                    match t.action {
                        crate::activate::outcome::Action::Locked { level } => format!("[Locked - {}]", crate::activate::outcome::lock_text(level)),
                        _ => "Locked".into(),
                    }
                }),
                locked: self
                    .target
                    .as_ref()
                    .and_then(|t| t.0.as_ref())
                    .is_some_and(|t| matches!(t.action, crate::activate::outcome::Action::Locked { .. })),
                // Only the two captured heights; further lines/wrapping await text
                // layout [hypothesis: hud.md §14.3; design: widths deferred].
                height: if p.name.lines().count() > 1 { 80.0 } else { 42.0 },
            }),
            marks: self.compass.as_ref().map_or_else(Vec::new, |view| {
                view.0
                    .iter()
                    .map(|(kind, offset)| {
                        use crate::compass::Mark;
                        use fnv_game::ui_hud::MarkKind;
                        fnv_game::ui_hud::CompassMark {
                            kind: match kind {
                                Mark::Actor => MarkKind::Actor,
                                Mark::Hostile => MarkKind::Hostile,
                                Mark::Place => MarkKind::Place,
                                Mark::Found => MarkKind::Found,
                                Mark::Quest => MarkKind::Quest,
                            },
                            heading: 139.0 + f64::from(*offset), // [observed: hud.md §14.5].
                            distance: 1.0,                       // [hypothesis: hud.md §14.6; drawing stores no distances].
                        }
                    })
                    .collect()
            }),
        };
        fnv_game::ui_hud::Texts {
            radiation_word: crate::radiation::WORD.into(),
            radiation_value: crate::radiation::value_text(game.radiation().resisted),
            xp_label: crate::xp_meter::XP_WORD.into(),
            xp_level_up: crate::xp_meter::LEVEL_UP_WORD.into(),
            needs: crate::hud_needs::texts(&self.sim),
            sneak: shown.sneak.then(|| crate::hud::sneak_now(game).word().to_string()),
            prompt_key: prompt.as_ref().map(|p| p.key_text().to_string()),
            prompt_verb: prompt.map(|p| if frame.prompt.as_ref().is_some_and(|p| p.locked) { "Open".into() } else { p.action }),
            frame,
        }
    }
}

/// What the message box and the prompt drawn from tiles were last written from (T15a,
/// `menus`): the menu's tile (a new one when it is built again) and what it showed, so a
/// frame with nothing changed writes nothing [design: the maintainer's review of #902].
#[derive(Default)]
struct MenusWritten {
    message: Option<(menus::BoxShown, Option<usize>)>,
    quantity: Option<(fnv_ui::TileId, menus::Amount)>,
    /// T15b: the Wait / Sleep menu.
    sleep_wait: Option<wait_menu::Shown>,
    /// The text sizes written, the message box's, the prompt's and the Wait menu's.
    sizes: [menus::SizesWritten; 3],
    /// [design] Each fallback reason is warned once per session, including after reopen.
    fallbacks: std::collections::HashSet<String>,
}

impl MenusWritten {
    fn warn_fallback(&mut self, menu: &str, status: menus::Tiled) {
        if let menus::Tiled::No(reason) = status {
            let line = format!("{menu} drawn by hand: {reason}");
            if self.fallbacks.insert(line.clone()) {
                warn!("{line}");
            }
        }
    }
}

/// T15a: what the message box and the prompt show, from their hand-built models.
#[derive(SystemParam)]
struct MenuTiles<'w, 's> {
    panel: Option<Res<'w, crate::message_menu::MessagePanel>>,
    active: Option<Res<'w, crate::menu::ActiveMenu>>,
    /// T15b: the Wait / Sleep menu and its words.
    wait: Option<Res<'w, crate::wait::WaitMenu>>,
    words: Option<Res<'w, crate::wait::model::Words>>,
    written: Local<'s, MenusWritten>,
}

impl MenuTiles<'_, '_> {
    /// The "How many?" prompt of the container, barter and inventory menus, when open.
    fn quantity(&self) -> Option<crate::menu::model::Quantity> {
        self.active.as_ref().and_then(|a| a.0.as_ref()).and_then(|m| m.quantity)
    }

    /// Write the switched-on menus' traits (`menus::write_message`, `menus::write_quantity`)
    /// when what they show changed, and their text tiles' sizes every frame they are drawn
    /// (each measured again only when its string changed), before scripts [design:
    /// ui-tile-tree.md §4.1]. Off, or without the menu files, nothing.
    fn write(&mut self, game: &mut fnv_game::Game<'_>, switch: vanilla::TileMenus, fonts: &mut draw::FontKit, calendar: Option<fnv_sim::clock::Calendar>) {
        if !(switch.message || switch.quantity || switch.sleep_wait) || game.ui_files.is_none() {
            return;
        }
        self.write_wait(game, switch, fonts, calendar);
        // [design] Reconsider an overflowing box when its view changes: using the final
        // tiled decision here would prevent a later short box from being written.
        let message_open = switch.message && game.menu == Some(fnv_game::game::menus::MESSAGE);
        let message = (message_open && menus::message_built(&game.ui_tree).yes()).then(|| game.ui_tree.menu(menus::MESSAGE_MENU)).flatten();
        let quantity = self.quantity().filter(|_| menus::quantity_tiled(switch.quantity, game, true));
        let prompt = quantity.and_then(|q| game.ui_tree.menu(menus::QUANTITY_MENU).map(|m| (m, q)));
        // Which showing the box on screen is (counted up for every box shown, one alike
        // included): its rows are made again for another (`menus::another_box`).
        let showing = game.front_box().map(|b| b.serial);
        let written = &mut *self.written;
        let ui = &mut game.ui_tree;
        match (message, self.panel.as_ref().and_then(|p| p.view.as_ref().map(|v| (v, p.framed())))) {
            (Some(menu), Some((view, selected))) => {
                let now = menus::BoxShown { menu, showing, view: view.clone() };
                let was = written.message.as_ref();
                let another = menus::another_box(was.map(|(b, _)| b), &now);
                if another || was.is_some_and(|(_, s)| *s != selected) {
                    let size = fonts.menu_measure(&menus::fonts(ui, menu));
                    menus::write_message(ui, view, selected, another, &size);
                    menus::write_text_sizes(ui, menu, &mut written.sizes[0], &size);
                    if another && menus::message_status(ui).yes() {
                        info!("{}", menus::describe_message(ui, view));
                    }
                    written.message = Some((now, selected));
                } else {
                    // One walk of the menu's texts a frame, for their fonts and sizes.
                    let texts = menus::texts(ui, menu);
                    let size = fonts.menu_measure(&menus::fonts_of(ui, &texts));
                    menus::write_text_sizes_of(ui, &texts, &mut written.sizes[0], &size);
                }
            }
            _ => written.message = None,
        }
        if message_open {
            written.warn_fallback(menus::MESSAGE_MENU, menus::message_status(ui));
        }
        if switch.quantity && self.active.as_ref().and_then(|a| a.0.as_ref()).is_some_and(|m| m.quantity.is_some()) {
            written.warn_fallback(menus::QUANTITY_MENU, menus::quantity_built(ui));
        }
        match prompt {
            Some((menu, q)) => {
                let amount = menus::Amount { amount: q.amount, most: q.max };
                if written.quantity != Some((menu, amount)) {
                    menus::write_quantity(ui, amount);
                    // The amount's text follows from the meter: worked out before its size.
                    ui.update();
                }
                let texts = menus::texts(ui, menu);
                let size = fonts.menu_measure(&menus::fonts_of(ui, &texts));
                menus::write_text_sizes_of(ui, &texts, &mut written.sizes[1], &size);
                if written.quantity != Some((menu, amount)) {
                    info!("{}", menus::describe_quantity(ui));
                    written.quantity = Some((menu, amount));
                }
            }
            None => written.quantity = None,
        }
    }

    /// T15b: write the Wait / Sleep menu's traits (`wait_menu::write`) when what it shows
    /// changed, and its text tiles' sizes every frame it is drawn, before scripts.
    fn write_wait(&mut self, game: &mut fnv_game::Game<'_>, switch: vanilla::TileMenus, fonts: &mut draw::FontKit, calendar: Option<fnv_sim::clock::Calendar>) {
        let written = &mut *self.written;
        let open = switch.sleep_wait && game.menu == Some(crate::script::MENU_SLEEP_WAIT);
        if open {
            written.warn_fallback(wait_menu::SLEEP_WAIT_MENU, wait_menu::built(&game.ui_tree));
        }
        let model = self.wait.as_ref().and_then(|w| w.0.as_ref());
        let (Some(model), Some(words), Some(cal), true) = (model, self.words.as_ref(), calendar, open && wait_menu::tiled(true, game)) else {
            written.sleep_wait = None;
            return;
        };
        let Some(menu) = game.ui_tree.menu(wait_menu::SLEEP_WAIT_MENU) else { return };
        let now = wait_menu::Shown { menu, view: model.view(words, &cal) };
        let ui = &mut game.ui_tree;
        let changed = written.sleep_wait.as_ref() != Some(&now);
        if changed {
            wait_menu::write(ui, &now.view);
            ui.update();
        }
        let texts = menus::texts(ui, menu);
        let size = fonts.menu_measure(&menus::fonts_of(ui, &texts));
        menus::write_text_sizes_of(ui, &texts, &mut written.sizes[2], &size);
        if changed {
            info!("{}", wait_menu::describe(ui));
            written.sleep_wait = Some(now);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn write(
    mut script: ResMut<ScriptState>,
    files: MenuFiles,
    hud: HudTiles,
    mut fonts: draw::FontKit,
    menus: Res<vanilla::TileMenus>,
    time: Res<Time>,
    mut animation: Local<fnv_game::ui_hud::Animation>,
    mut told: Local<[Option<String>; 4]>,
    mut menu_tiles: MenuTiles,
    mut mods_entry: ResMut<crate::pause::ModsEntry>,
    mut start_names: Local<mods::KeptNames>,
) {
    let Some(rt) = script.rt.as_mut() else { return };
    // T15a: the prompt over the container, barter and inventory menus builds `QuantityMenu`
    // while it shows (`Game::ui_prompt`), whether or not it is drawn from it, as the open
    // menus build (T3b).
    rt.game.ui_prompt = menu_tiles.quantity().map(|_| fnv_game::game::menus::QUANTITY);
    files.write(&mut rt.game);
    // T11: the pause column's Mods entry, while a mod's tiles are in the start menu.
    let ui_files = rt.game.ui_files.as_deref().map(|f| f as &dyn fnv_ui::Source);
    let mods_now = crate::pause::ModsEntry(mods::available(*menus, &rt.game.ui_tree, ui_files, &mut start_names));
    if *mods_entry != mods_now {
        info!("tiles: {}", if mods_now.0 { "a mod's tiles in StartMenu: the pause menu has a Mods entry" } else { "no Mods entry in the pause menu" });
        *mods_entry = mods_now;
    }
    menu_tiles.write(&mut rt.game, *menus, &mut fonts, hud.sim.world.calendar().ok());
    let shown = hud.shown(&rt.game, &files);
    let mut texts = hud.texts(&mut rt.game, shown);
    if let Some(root) = rt.game.ui_tree.find_tile("HUDMainMenu") {
        // [design] Reset clocks on tree rebuild/save-load, without touching the hand HUD.
        if rt.game.ui_tree.tile(root).get("_NVFAnimationClock").is_none() {
            *animation = Default::default();
            rt.game.ui_tree.set_trait(root, "_NVFAnimationClock", Value::Num(1.0));
        }
        texts.frame.animation = animation.step(f64::from(time.delta_secs()), shown.root && shown.corners, fnv_game::experience::standing(&rt.game), texts.frame.crippled);
    }
    // Who reads the HUD's traits: the tile-drawn HUD, mod tiles in it (their traits may
    // copy the HUD's), or a script that called a UI function. With none (the default
    // game), the writes wait for the first UI call (`ui_hud::flush`) and the frame does
    // no tree work [design: maintainer's review of #902]. The widths are kept back with
    // the strings (their measure), so the first UI call reads them as a frame writes
    // them. The corners are written only with the tile-drawn HUD, and nothing waits then.
    let ui = &rt.game.ui_tree;
    let mod_tiles = ui.menu(draw::HUD_MENU).is_some_and(|m| ui.has_mod_added_tiles(m));
    if !(menus.hud || mod_tiles || rt.game.ui_used) {
        let widths = if rt.game.ui_files.is_some() { fonts.widths(&rt.game.ui_tree) } else { None };
        fnv_game::ui_hud::defer(&mut rt.game, shown, texts, widths);
        return;
    }
    rt.game.ui_hud_pending = None;
    fnv_game::ui_hud::write(&mut rt.game.ui_tree, shown, &texts);
    // The text tiles' widths from the strings just written, before scripts read them
    // [design: ui-tile-tree.md §4.1; observed widths: hud.md §14.1].
    if rt.game.ui_files.is_some() {
        fonts.write_widths(&mut rt.game.ui_tree);
        if menus.hud {
            let corner = hud.corner(shown, &mut fonts);
            vanilla::write_corners(&mut rt.game.ui_tree, &corner);
            // The messages, quest notices and subtitles, only while the switch is on: off,
            // nothing is made under their pieces [design].
            let shows = hud.notices(&rt.game, shown);
            notices::write(&mut rt.game.ui_tree, &shows, &fonts.hud_lines());
            // Their widths from the strings just written, before scripts read them, as
            // the other HUD texts' above.
            fonts.write_notice_widths(&mut rt.game.ui_tree);
            for (line, last) in notices::describe(&shows).into_iter().zip(told.iter_mut()) {
                if line.is_some() && line != *last {
                    info!("{}", line.as_deref().unwrap_or_default());
                }
                *last = line;
            }
            // The breath meter: shown under the hand-built meter's rule, at the breath used
            // (`vanilla::write_breath`, hud.md §14.12).
            let used = hud.breath.as_ref().map_or(0.0, |b| 1.0 - f64::from(b.share()));
            vanilla::write_breath(&mut rt.game.ui_tree, shown.breath, used);
        }
    }
}

/// Works the tree out after the scripts; nothing to do on a frame where nothing changed
/// it (`fnv_ui::Ui::update`). The scripts' gradual changes step first, on real time
/// (`fnv_game::ui_gradients`, task T9).
fn update(mut script: ResMut<ScriptState>, time: Res<Time<Real>>, mut wait_check: ResMut<input::WaitSliderCheck>) {
    let Some(rt) = script.rt.as_mut() else { return };
    rt.game.ui_gradients.step(&mut rt.game.ui_tree, f64::from(time.delta_secs()));
    if rt.game.ui_files.is_some() {
        rt.game.ui_tree.update();
        input::check_wait_drawn(&rt.game.ui_tree, &mut wait_check);
        // Once, if something changes the tree's shape every frame (#1061).
        if let Some(line) = rt.game.ui_tree.take_reshape_warning() {
            warn!("{line}");
        }
    }
    fnv_game::ui_more::prune_attachments(&mut rt.game);
    // MenuTapKey's keys stay queued unless a menu takes tile input (`input::feed`) [design].
}

pub struct TilesPlugin;

impl Plugin for TilesPlugin {
    fn build(&self, app: &mut App) {
        // Writes run after physics, before activation's scripts and PlaySet::Scripts;
        // evaluation runs after PlaySet::Scripts, before the menus draw [design: ui-tile-tree.md §4.1].
        // The mod tiles draw with the menus, after evaluation [design: §4.1 step 4].
        // T12a: `[UI] TileMenus=` read at start; the hand-built pieces the tiles draw are
        // hidden after every drawing system, before visibility is passed down [design].
        vanilla::setup(app);
        input::setup(app);
        app.init_resource::<draw::TileFonts>()
            .add_systems(Update, write.after(PlaySet::Physics).after(crate::compass::place).before(PlaySet::Activation))
            // T8: the pointer and keys after the engine's writes, before scripts (§4.1 step 1).
            .add_systems(Update, input::feed.after(write).before(PlaySet::Activation))
            .add_systems(Update, update.after(PlaySet::Scripts).before(PlaySet::Ui))
            .add_systems(Update, (draw::draw, draw::scroll, model::animate).chain().in_set(PlaySet::Ui))
            .add_systems(PostUpdate, vanilla::hide_hand_built.before(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_ui::{MemSource, Ui};

    #[test]
    fn fallback_reasons_are_warned_once_across_reopens() {
        // [design] Reopening the same broken file or an oversized box cannot spam logs;
        // a different reason or menu still gets its own warning.
        let mut written = MenusWritten::default();
        for _ in 0..2 {
            written.warn_fallback(menus::MESSAGE_MENU, menus::Tiled::No("path missing".into()));
            written.warn_fallback(menus::MESSAGE_MENU, menus::Tiled::Yes);
        }
        assert_eq!(written.fallbacks.len(), 1);
        written.warn_fallback(menus::QUANTITY_MENU, menus::Tiled::No("path missing".into()));
        written.warn_fallback(menus::MESSAGE_MENU, menus::Tiled::No("button rows exceed LIST_MOST (420); scrolling is not built".into()));
        assert_eq!(written.fallbacks.len(), 3);
    }

    /// [design] A tile placed by `screen()`, as the T15 menus centre their boxes.
    fn centred() -> Ui {
        let src = MemSource::default().with(
            "m.xml",
            r#"<menu name="M"><rect name="Box"><width>100</width>
              <x><copy src="screen()" trait="width"/><sub src="me()" trait="width"/><div>2</div></x>
              <_thick><copy>2</copy><mul src="screen()" trait="resolutionconverter"/></_thick></rect></menu>"#,
        );
        let mut ui = Ui::new();
        ui.load_menu(&src, "m.xml").unwrap();
        ui
    }

    #[test]
    fn screen_keeps_the_windows_pixels_unless_a_t15_menu_is_on() {
        // Off (the default game, or the tile HUD alone): the window's size, no converter,
        // and a tile placed by screen() where it was before T15a.
        assert!(!interface_screen(vanilla::TileMenus::default()));
        assert!(!interface_screen(vanilla::TileMenus { hud: true, ..Default::default() }));
        let mut ui = centred();
        write_screen(&mut ui, 1280.0, 720.0, false);
        ui.update();
        let s = ui.tile(ui.screen());
        assert_eq!((s.get("width").map(Value::num), s.get("height").map(Value::num)), (Some(1280.0), Some(720.0)));
        assert!(s.get("resolutionconverter").is_none());
        assert_eq!(ui.get_float("M/Box/x"), Some(590.0));
        assert_eq!(ui.get_float("M/Box/_thick"), Some(0.0));
        // On: interface units, 960 high, and the converter.
        assert!(interface_screen(vanilla::TileMenus { message: true, ..Default::default() }));
        assert!(interface_screen(vanilla::TileMenus { quantity: true, ..Default::default() }));
        assert!(interface_screen(vanilla::TileMenus { start: true, ..Default::default() }));
        assert!(interface_screen(vanilla::TileMenus { sleep_wait: true, ..Default::default() }));
        write_screen(&mut ui, 1280.0, 720.0, true);
        ui.update();
        assert_eq!(ui.get_float("M/Box/x").map(|x| (x * 100.0).round() / 100.0), Some(803.33));
        // A line 2 pixels thick: 2 × 960/720 units.
        assert_eq!(ui.get_float("M/Box/_thick"), Some(2.0 * 960.0 / 720.0));
        assert_eq!(screen_size(1280.0, 720.0).1, 960.0);
    }
}
