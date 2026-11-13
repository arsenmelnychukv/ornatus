//! Orbit camera with damping (decision 0005: orbit never fights placement).

use glam::{Mat4, Vec2, Vec3};

use crate::pick::Ray;

const MAX_PITCH: f32 = 1.55;
const MIN_DISTANCE: f32 = 0.2;
const MAX_DISTANCE: f32 = 20.0;
/// How quickly leftover orbit speed fades after the user lets go, per second.
const DAMPING: f32 = 6.0;

/// A camera that looks at a target from a point on a sphere.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbitCamera {
    /// Point the camera looks at.
    pub target: Vec3,
    /// Distance from the target.
    pub distance: f32,
    /// Rotation around the vertical axis, in radians.
    pub yaw: f32,
    /// Rotation above the horizon, in radians.
    pub pitch: f32,
    /// Vertical field of view, in radians.
    pub fov_y: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            distance: 3.5,
            yaw: 0.6,
            pitch: 0.35,
            fov_y: 45.0_f32.to_radians(),
        }
    }
}

impl OrbitCamera {
    /// Camera position in world space.
    pub fn eye(&self) -> Vec3 {
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        self.target + self.distance * Vec3::new(sin_yaw * cos_pitch, sin_pitch, cos_yaw * cos_pitch)
    }

    /// World to camera space.
    pub fn view(&self) -> Mat4 {
        glam::camera::rh::view::look_at_mat4(self.eye(), self.target, Vec3::Y)
    }

    /// Camera to clip space, with wgpu's 0..1 depth range.
    pub fn projection(&self, aspect: f32) -> Mat4 {
        let far = self.distance + 50.0;
        glam::camera::rh::proj::directx::perspective(self.fov_y, aspect.max(0.01), 0.05, far)
    }

    /// World to clip space.
    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        self.projection(aspect) * self.view()
    }

    /// The ray through a point of the viewport. `ndc` runs from (-1, -1) at
    /// the bottom left to (1, 1) at the top right.
    pub fn ray(&self, ndc: Vec2, aspect: f32) -> Ray {
        let inverse = self.view_projection(aspect).inverse();
        let far = inverse.project_point3(ndc.extend(1.0));
        Ray::new(self.eye(), far - self.eye())
    }

    /// Rotate by the given angles, keeping the camera from flipping over.
    pub fn orbit(&mut self, delta_yaw: f32, delta_pitch: f32) {
        self.yaw = (self.yaw + delta_yaw).rem_euclid(std::f32::consts::TAU);
        self.pitch = (self.pitch + delta_pitch).clamp(-MAX_PITCH, MAX_PITCH);
    }

    /// Move closer (`amount` > 0) or further (`amount` < 0). Each unit changes
    /// the distance by about ten percent.
    pub fn zoom(&mut self, amount: f32) {
        self.distance = (self.distance * (-amount * 0.1).exp()).clamp(MIN_DISTANCE, MAX_DISTANCE);
    }

    /// Slide the target in the camera plane. `dx` and `dy` are fractions of
    /// the viewport height.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        let view = self.view().inverse();
        let right = view.x_axis.truncate();
        let up = view.y_axis.truncate();
        let height = 2.0 * self.distance * (self.fov_y / 2.0).tan();
        self.target += (-right * dx + up * dy) * height;
    }
}

/// An orbit camera plus the leftover speed that gives it a damped feel.
#[derive(Debug, Clone, Copy, Default)]
pub struct CameraController {
    /// The camera being driven.
    pub camera: OrbitCamera,
    yaw_speed: f32,
    pitch_speed: f32,
}

impl CameraController {
    /// A controller around an existing camera.
    pub fn new(camera: OrbitCamera) -> Self {
        Self {
            camera,
            yaw_speed: 0.0,
            pitch_speed: 0.0,
        }
    }

    /// The user dragged by this much over `dt` seconds.
    pub fn orbit_input(&mut self, delta_yaw: f32, delta_pitch: f32, dt: f32) {
        self.camera.orbit(delta_yaw, delta_pitch);
        if dt > f32::EPSILON {
            self.yaw_speed = delta_yaw / dt;
            self.pitch_speed = delta_pitch / dt;
        }
    }

    /// Advance the damping. Call once per frame; pass `true` when the user
    /// is still dragging, so no extra motion is added.
    pub fn update(&mut self, dt: f32, dragging: bool) {
        if dragging {
            return;
        }
        self.camera
            .orbit(self.yaw_speed * dt, self.pitch_speed * dt);
        let keep = (-DAMPING * dt).exp();
        self.yaw_speed *= keep;
        self.pitch_speed *= keep;
        if self.yaw_speed.abs() < 1e-3 && self.pitch_speed.abs() < 1e-3 {
            self.yaw_speed = 0.0;
            self.pitch_speed = 0.0;
        }
    }

    /// True while leftover motion is still playing out.
    pub fn is_moving(&self) -> bool {
        self.yaw_speed != 0.0 || self.pitch_speed != 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eye_is_at_the_camera_distance() {
        let camera = OrbitCamera {
            target: Vec3::new(1.0, 2.0, 3.0),
            ..OrbitCamera::default()
        };
        assert!(((camera.eye() - camera.target).length() - camera.distance).abs() < 1e-5);
    }

    #[test]
    fn target_projects_to_the_screen_center() {
        let camera = OrbitCamera::default();
        let clip = camera.view_projection(16.0 / 9.0) * camera.target.extend(1.0);
        let ndc = clip.truncate() / clip.w;
        assert!(ndc.x.abs() < 1e-5 && ndc.y.abs() < 1e-5);
        assert!((0.0..1.0).contains(&ndc.z), "depth was {}", ndc.z);
    }

    #[test]
    fn center_ray_points_at_the_target() {
        let camera = OrbitCamera::default();
        let ray = camera.ray(Vec2::ZERO, 1.5);
        let expected = (camera.target - camera.eye()).normalize();
        assert!(
            (ray.dir - expected).length() < 1e-4,
            "{:?} vs {expected}",
            ray.dir
        );
        assert!((ray.origin - camera.eye()).length() < 1e-6);
    }

    #[test]
    fn corner_rays_leave_through_the_matching_corner() {
        let camera = OrbitCamera::default();
        let aspect = 2.0;
        // A ray through the top right must project back to the top right.
        let ray = camera.ray(Vec2::new(1.0, 1.0), aspect);
        let clip = camera.view_projection(aspect) * ray.at(3.0).extend(1.0);
        let ndc = clip.truncate() / clip.w;
        assert!(
            (ndc.x - 1.0).abs() < 1e-3 && (ndc.y - 1.0).abs() < 1e-3,
            "{ndc}"
        );
    }

    #[test]
    fn pitch_is_clamped() {
        let mut camera = OrbitCamera::default();
        camera.orbit(0.0, 100.0);
        assert!(camera.pitch <= MAX_PITCH);
        camera.orbit(0.0, -200.0);
        assert!(camera.pitch >= -MAX_PITCH);
    }

    #[test]
    fn zoom_is_clamped() {
        let mut camera = OrbitCamera::default();
        camera.zoom(1000.0);
        assert_eq!(camera.distance, MIN_DISTANCE);
        camera.zoom(-1000.0);
        assert_eq!(camera.distance, MAX_DISTANCE);
    }

    #[test]
    fn pan_moves_the_target_but_not_the_distance() {
        let mut camera = OrbitCamera::default();
        let before = camera.target;
        camera.pan(0.1, 0.0);
        assert!((camera.target - before).length() > 0.01);
        assert!(((camera.eye() - camera.target).length() - camera.distance).abs() < 1e-4);
    }

    #[test]
    fn damping_coasts_then_stops() {
        let mut controller = CameraController::default();
        controller.orbit_input(0.1, 0.0, 1.0 / 60.0);
        let yaw_after_input = controller.camera.yaw;
        assert!(controller.is_moving());
        for _ in 0..600 {
            controller.update(1.0 / 60.0, false);
        }
        assert!(!controller.is_moving());
        assert_ne!(controller.camera.yaw, yaw_after_input);
    }

    #[test]
    fn no_coasting_while_dragging() {
        let mut controller = CameraController::default();
        controller.orbit_input(0.1, 0.0, 1.0 / 60.0);
        let yaw = controller.camera.yaw;
        controller.update(1.0 / 60.0, true);
        assert_eq!(controller.camera.yaw, yaw);
    }
}
