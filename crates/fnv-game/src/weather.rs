//! The weather scripts ask for and the one on the screen (docs/design/script-stubs.md
//! §3): SetWeather, ForceWeather, ReleaseWeatherOverride, IsRaining and
//! GetIsCurrentWeather. The sky (`nvforever` `sky.rs`) takes the request and tells
//! back which weather it shows ([`Game::set_shown_weather`]).

use crate::functions::{none, yes, Args, Ctx};
use crate::game::Game;
use fnv_script::vm::Value;
use fnv_sim::FormId;

/// A weather a script asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeatherRequest {
    pub weather: FormId,
    /// ForceWeather: at once, without the change's fade; SetWeather: the normal change.
    pub instant: bool,
    /// The override flag: the climate does not change the weather again until
    /// ReleaseWeatherOverride.
    pub hold: bool,
}

/// The weather as scripts set it and as the engine shows it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WeatherState {
    /// The latest request, until the sky takes it.
    pub request: Option<WeatherRequest>,
    /// ReleaseWeatherOverride was called and the sky has not taken it yet.
    pub release: bool,
    /// The weather the sky shows (its form), as the engine last told.
    pub shown: Option<FormId>,
    /// The weather shown is classified rainy and the change to it is over.
    pub raining: bool,
    /// The weather shown before the one shown now (ClimateControl's
    /// GetOutgoingWeather), as the engine told the change.
    pub outgoing: Option<FormId>,
}

impl Game<'_> {
    /// The sky takes the script's weather request, if one is waiting.
    pub fn take_weather_request(&mut self) -> Option<WeatherRequest> {
        self.weather.request.take()
    }

    /// The sky takes a release of the override, if one is waiting.
    pub fn take_weather_release(&mut self) -> bool {
        std::mem::take(&mut self.weather.release)
    }

    /// The sky tells which weather it shows, and whether it is a rainy one fully faded
    /// in.
    pub fn set_shown_weather(&mut self, weather: FormId, raining: bool) {
        if self.weather.shown.is_some_and(|w| w != weather) {
            self.weather.outgoing = self.weather.shown;
        }
        self.weather.shown = Some(weather);
        self.weather.raining = raining;
    }
}

fn request(g: &mut Game, a: &Args, instant: bool) -> Result<Value, String> {
    let weather = a.form(0)?;
    if g.data.sigs.get(&weather).is_some_and(|s| s != b"WTHR") {
        return Err(format!("{} is not a weather", g.data.name(weather)));
    }
    let hold = a.num_or(1, 0.0) != 0.0;
    g.weather.request = Some(WeatherRequest { weather, instant, hold });
    if hold {
        g.weather.release = false;
    }
    none()
}

/// `SetWeather weather [override]`: the weather changes to it the normal way (its fade);
/// with the override flag the weather stays until ReleaseWeatherOverride, else the
/// weather system changes it again as usual [documented: GECK wiki SetWeather].
pub fn set_weather(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    request(g, a, false)
}

/// `ForceWeather weather [override]`: as SetWeather, at once, skipping the change
/// [documented: GECK wiki ForceWeather].
pub fn force_weather(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    request(g, a, true)
}

/// `ReleaseWeatherOverride`: the weather follows the weather system again after a
/// SetWeather or ForceWeather with the override flag [documented: GECK wiki
/// ReleaseWeatherOverride].
pub fn release_weather_override(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    g.weather.release = true;
    none()
}

/// `IsRaining`: 1 when the current weather is classified rainy and stable (its change
/// over), else 0 [documented: GECK wiki IsRaining: "cannot return a value different from
/// 0 and 1"]. The classification is WTHR DATA's Rainy flag (`fnv_world::sky::Weather`);
/// before the sky has told any weather (a headless run, an interior from the start) it
/// reads 0.
pub fn is_raining(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    yes(g.weather.raining)
}

/// `GetIsCurrentWeather weather`: 1 when that weather is the current one [documented:
/// GECK wiki GetIsCurrentWeather]: the one the sky shows, or, before the sky has taken
/// it, the one a script just set [hypothesis: the change is current from the call].
pub fn get_is_current_weather(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let w = a.form(0)?;
    let current = g.weather.request.map(|r| r.weather).or(g.weather.shown);
    yes(current == Some(w))
}

/// The host's first weather for an exterior place: (worldspace, position) -> weather.
pub type StartWeather = std::sync::Arc<dyn Fn(FormId, [f32; 3]) -> Option<FormId> + Send + Sync>;

static START: std::sync::Mutex<Option<StartWeather>> = std::sync::Mutex::new(None);

/// Set the first-weather hook for a run without the engine's sky (fnvtool).
pub fn set_start_weather(f: StartWeather) {
    if let Ok(mut g) = START.lock() {
        *g = Some(f);
    }
}

impl Game<'_> {
    /// Without the engine's sky, the weather of the exterior the player is now in, from
    /// the host's hook (the runner's go_to_cell) [hypothesis: the sky's first weather
    /// there]. Nothing when the sky tells the weather itself or no hook is set.
    pub fn settle_start_weather(&mut self) {
        let Some(hook) = START.lock().ok().and_then(|g| g.clone()) else { return };
        let Some(ws) = self.world.worldspace_of(FormId::PLAYER_REF) else { return };
        let Some(pos) = self.world.peek(FormId::PLAYER_REF).map(|p| p.pos) else { return };
        if let Some(w) = hook(ws, pos) {
            self.weather.shown = Some(w);
        }
    }
}
