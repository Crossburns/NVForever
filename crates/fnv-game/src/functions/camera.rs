//! The console's camera functions (docs/design/console.md, Camera commands): the free
//! camera, its speed, the field of view and the HUD. The game keeps the request
//! ([`CameraRequests`], `Game::ui.camera`); the engine's camera takes it
//! (`nvf_play::free_cam`).

use super::{none, Args, Ctx};
use crate::game::Game;
use fnv_script::vm::Value;

/// What the console asked of the camera, for the engine to take.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CameraRequests {
    /// ToggleFlyCam calls not taken yet, each with its mode (0 when none was given).
    pub fly_toggles: Vec<i32>,
    /// SetUFOCamSpeedMult: the free camera's speed multiplier (`None`: never set, 1).
    pub fly_speed_mult: Option<f32>,
    /// A SetCameraFOV call not taken yet: the world's and the first-person field of view
    /// in degrees, `None` for the default.
    pub fov: Option<(Option<f32>, Option<f32>)>,
    /// The last SetCameraFOV's values, kept after the engine took them (Anh's
    /// GetCameraFOV); `None` for the default.
    pub fov_set: (Option<f32>, Option<f32>),
    /// ToggleMenus: the HUD, the crosshair and the corner messages are hidden.
    pub menus_hidden: bool,
    /// The fly camera is on, by the ToggleFlyCam calls so far (GetTFC).
    pub fly_on: bool,
}

/// `ToggleFlyCam [mode]` (`tfc`): the camera leaves the player and flies, or comes back
/// [documented: GECK wiki ToggleFlyCam, through the catalogue]. The modes the engine
/// knows are in docs/design/console.md.
pub fn toggle_fly_cam(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.ui.camera.fly_toggles.push(a.num_or(0, 0.0) as i32);
    g.ui.camera.fly_on = !g.ui.camera.fly_on;
    none()
}

/// `SetUFOCamSpeedMult mult` (`sucsm`): the free camera's speed multiplier
/// [documented: GECK wiki SetUFOCamSpeedMult, through the catalogue]. Below 0 is taken
/// as 0 [decision].
pub fn set_ufo_cam_speed_mult(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.ui.camera.fly_speed_mult = Some(a.num(0)?.max(0.0) as f32);
    none()
}

/// One SetCameraFOV value [documented: GECK wiki SetCameraFOV, through the catalogue]:
/// omitted or 0 is the default, a negative value becomes the reciprocal of its size, and
/// both are capped at 160.
pub fn fov_value(v: Option<f64>) -> Option<f32> {
    let v = v.filter(|v| *v != 0.0)?;
    let v = if v < 0.0 { 1.0 / -v } else { v };
    Some(v.min(160.0) as f32)
}

/// `SetCameraFOV [world] [first person]` (`fov`): the fields of view, in degrees.
pub fn set_camera_fov(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let arg = |i: usize| a.get(i).and_then(|v| v.value()).map(|v| v.num());
    g.ui.camera.fov = Some((fov_value(arg(0)), fov_value(arg(1))));
    g.ui.camera.fov_set = (fov_value(arg(0)), fov_value(arg(1)));
    none()
}

/// `ToggleMenus` (`tm`): hides or shows the HUD [documented: GECK wiki ToggleMenus,
/// through the catalogue]; what is hidden is in docs/design/console.md.
pub fn toggle_menus(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    g.ui.camera.menus_hidden = !g.ui.camera.menus_hidden;
    tracing::info!("ToggleMenus: the HUD is {}", if g.ui.camera.menus_hidden { "hidden" } else { "shown" });
    none()
}

#[cfg(test)]
mod tests {
    use super::fov_value;

    #[test]
    fn fov_values_follow_the_catalogue() {
        assert_eq!(fov_value(None), None);
        assert_eq!(fov_value(Some(0.0)), None);
        assert_eq!(fov_value(Some(90.0)), Some(90.0));
        assert_eq!(fov_value(Some(200.0)), Some(160.0));
        assert_eq!(fov_value(Some(-4.0)), Some(0.25));
    }
}
