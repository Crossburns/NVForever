//! Where the machine sits in front of the player's camera so that it fills the screen as
//! in the original (docs/spec-notes/vigor-tester.md §5).
//!
//! The original's view of the model, fitted to the owner's 1280 × 720 screenshots (7.jpg
//! and 10.jpg: ten points of the label boards, the poster, the lever and the two arrow
//! buttons, 7 px RMS) [observed: fitted]: a camera at model point (0, 78.4, 98.9)
//! looking along −y, tipped 9.15° down, with a vertical field of view of 48.4°. The
//! model's +z is up and its −x is the screen's right.
//!
//! NVForever places the machine below the player's own camera, given the fitted field of
//! view while the menu is open (`super`); the view's camera (nvforever `vigor_view.rs`)
//! sits on it and takes its field of view. Should the camera have another one, the
//! machine is moved along the view line until the board plane fills the same part of the
//! screen (the lever, nearer the eye, then comes out slightly smaller than in the
//! original when the view is wider). Then the whole is scaled down about
//! the eye by [`SCALE`] so that it stands nearer than the walls of the room around the
//! player and the first-person body [engineering choice; the near plane is 5 units].

use bevy::prelude::*;

/// The fitted camera (model space).
pub const EYE: Vec3 = Vec3::new(0.0, 78.4, 98.9);
pub const PITCH_DEG: f32 = 9.15;
pub const FIT_VFOV_DEG: f32 = 48.4;
/// Distance from [`EYE`] along the view to the board plane (y −10.9) at the screen's middle.
pub const BOARD_DISTANCE: f32 = 89.5;
/// How much smaller than its own size the machine is drawn, about the eye.
pub const SCALE: f32 = 0.13;

/// The view direction, its up and right, in model space.
pub fn basis() -> (Vec3, Vec3, Vec3) {
    let p = PITCH_DEG.to_radians();
    let fwd = Vec3::new(0.0, -p.cos(), -p.sin());
    let up = Vec3::new(0.0, -p.sin(), p.cos());
    (fwd, up, fwd.cross(up))
}

/// The model's transform below a camera whose vertical field of view is `vfov`
/// (radians): model space to the camera's own space (looking along −z, +y up).
pub fn model_in_camera(vfov: f32) -> Transform {
    let (fwd, up, right) = basis();
    let ratio = (FIT_VFOV_DEG.to_radians() / 2.0).tan() / (vfov.clamp(0.2, 3.0) / 2.0).tan();
    let eye = EYE + fwd * BOARD_DISTANCE * (1.0 - ratio);
    // Rows: right, up, back (−fwd).
    let m = Mat3::from_cols(Vec3::new(right.x, up.x, -fwd.x), Vec3::new(right.y, up.y, -fwd.y), Vec3::new(right.z, up.z, -fwd.z));
    Transform { translation: -SCALE * (m * eye), rotation: Quat::from_mat3(&m).normalize(), scale: Vec3::splat(SCALE) }
}

/// How far in front of the camera the board plane is drawn (the lights stand before it).
pub fn board_depth(vfov: f32) -> f32 {
    let ratio = (FIT_VFOV_DEG.to_radians() / 2.0).tan() / (vfov.clamp(0.2, 3.0) / 2.0).tan();
    BOARD_DISTANCE * ratio * SCALE
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pixel of a model point on a 1280 × 720 screen through a camera with `vfov`.
    fn pixel(p: Vec3, vfov: f32) -> Vec2 {
        let c = model_in_camera(vfov).transform_point(p);
        let f = 360.0 / (vfov / 2.0).tan();
        Vec2::new(640.0 + f * c.x / -c.z, 360.0 - f * c.y / -c.z)
    }

    #[test]
    fn the_fit_puts_the_parts_where_the_screenshots_have_them() {
        let v = FIT_VFOV_DEG.to_radians();
        // Label row 10 and row 1 (7.jpg: about (500, 85) and (505, 489)), the lever's +.
        let r10 = pixel(Vec3::new(15.6, -10.9, 114.7), v);
        let r1 = pixel(Vec3::new(15.6, -10.9, 69.3), v);
        let lever = pixel(Vec3::new(0.0, 6.7, 68.0), v);
        assert!((r10 - Vec2::new(500.0, 85.0)).length() < 10.0, "{r10}");
        assert!((r1 - Vec2::new(505.0, 489.0)).length() < 10.0, "{r1}");
        // The fit's own residual at the lever is 14 px.
        assert!((lever - Vec2::new(640.0, 576.0)).length() < 16.0, "{lever}");
        // The model's −x is the screen's right.
        assert!(pixel(Vec3::new(-20.0, -10.9, 90.0), v).x > 640.0);
    }

    #[test]
    fn a_wider_view_keeps_the_board_where_it_was() {
        let wide = 58.7f32.to_radians();
        for p in [Vec3::new(15.6, -10.9, 114.7), Vec3::new(-26.3, -10.9, 90.0)] {
            let a = pixel(p, FIT_VFOV_DEG.to_radians());
            let b = pixel(p, wide);
            // The boards stand upright and the view is tipped: only the middle of the
            // board plane keeps its place exactly.
            assert!((a - b).length() < 5.0, "{a} {b}");
        }
        // In front of the near plane (5 units) and behind it nothing.
        let near = model_in_camera(wide).transform_point(Vec3::new(0.0, 11.7, 58.7));
        assert!(-near.z > 6.0, "{near}");
        assert!((board_depth(wide) - 9.3).abs() < 0.5, "{}", board_depth(wide));
    }
}
