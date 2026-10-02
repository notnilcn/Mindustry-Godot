// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Launch/land cutscene state machine (`core/Renderer.java:180-196,552-586`,
//! plan 16 §3.10/M7). The concrete animator (curve/shadow/scale) is plan 07's
//! `LaunchAnimator`; this module owns `showLanding`/`showLaunch`/`landTime`/
//! `landScale`/`getLandTimeIn` and the `weatherAlpha` handoff to plan 17.
//! View-only.

/// The cutscene animator contract (plan 07's `LaunchAnimator`).
pub trait LaunchAnimator {
    /// `launchDuration()`.
    fn launch_duration(&self) -> f32;
    /// `beginLaunch(launching)`.
    fn begin_launch(&mut self, launching: bool);
    /// `updateLaunch()` (per tick, paused-aware).
    fn update_launch(&mut self);
    /// `zoomLaunch()`.
    fn zoom_launch(&self) -> f32;
    /// `endLaunch()`.
    fn end_launch(&mut self);
}

/// `Renderer`'s launch/land state (`landTime`, `launching`, `weatherAlpha`).
#[derive(Default)]
pub struct Cutscene {
    animator: Option<Box<dyn LaunchAnimator>>,
    launching: bool,
    land_time: f32,
    camerascale: f32,
    weather_alpha: f32,
}

impl Cutscene {
    /// Builds with the current camera scale.
    pub fn new(camerascale: f32) -> Self {
        Self {
            animator: None,
            launching: false,
            land_time: 0.0,
            camerascale,
            weather_alpha: 0.0,
        }
    }

    /// Escapes the borrow of the animator.
    fn take_animator(&mut self) -> Option<Box<dyn LaunchAnimator>> {
        self.animator.take()
    }

    /// `Renderer.isCutscene()`.
    pub fn is_cutscene(&self) -> bool {
        self.land_time > 0.0
    }

    /// `Renderer.isLaunching()`.
    pub fn is_launching(&self) -> bool {
        self.launching
    }

    /// `Renderer.getLandTime()`.
    pub fn land_time(&self) -> f32 {
        self.land_time
    }

    /// `Renderer.getDisplayScale()` (cutscene camera scale).
    pub fn camerascale(&self) -> f32 {
        self.camerascale
    }

    /// `Renderer.weatherAlpha`.
    pub fn weather_alpha(&self) -> f32 {
        self.weather_alpha
    }

    /// `Renderer.landScale()`.
    pub fn land_scale(&self) -> f32 {
        if self.land_time > 0.0 {
            self.camerascale
        } else {
            1.0
        }
    }

    /// `Renderer.getLandTimeIn()`.
    pub fn get_land_time_in(&self) -> f32 {
        let Some(anim) = self.animator.as_ref() else {
            return 0.0;
        };
        let mut fin = self.land_time / anim.launch_duration().max(1e-6);
        if !self.launching {
            fin = 1.0 - fin;
        }
        fin
    }

    /// `Renderer.showLanding(landCore)`.
    pub fn show_landing(&mut self, mut anim: Box<dyn LaunchAnimator>) {
        let duration = anim.launch_duration();
        anim.begin_launch(false);
        self.camerascale = anim.zoom_launch();
        self.animator = Some(anim);
        self.launching = false;
        self.land_time = duration;
    }

    /// `Renderer.showLaunch(landCore)`.
    pub fn show_launch(&mut self, mut anim: Box<dyn LaunchAnimator>) {
        let duration = anim.launch_duration();
        anim.begin_launch(true);
        self.animator = Some(anim);
        self.launching = true;
        self.land_time = duration;
    }

    /// `Renderer.update` cutscene/`weatherAlpha` block. `delta_frames` is
    /// `Time.delta` in frames (seconds × 60) for `lerpDelta` parity.
    pub fn update(&mut self, delta_frames: f32, paused: bool) {
        if self.animator.is_none() {
            self.land_time = 0.0;
        }
        if self.land_time > 0.0 {
            if !paused && let Some(anim) = self.animator.as_mut() {
                anim.update_launch();
            }
            self.weather_alpha = 0.0;
            if let Some(anim) = self.animator.as_ref() {
                self.camerascale = anim.zoom_launch();
            }
            if !paused {
                self.land_time -= delta_frames / 60.0;
            }
        } else {
            self.weather_alpha = lerp_delta(self.weather_alpha, 1.0, 0.08, delta_frames);
        }

        if self.animator.is_some()
            && self.land_time <= 0.0
            && let Some(mut anim) = self.take_animator()
        {
            anim.end_launch();
        }
    }
}

/// Arc `Mathf.lerpDelta` with an explicit frame delta.
fn lerp_delta(from: f32, to: f32, progress: f32, delta_frames: f32) -> f32 {
    from + (to - from) * (progress * delta_frames).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Mock {
        duration: f32,
        zoom: f32,
        ticks: usize,
        began: bool,
        ended: bool,
    }

    impl LaunchAnimator for Mock {
        fn launch_duration(&self) -> f32 {
            self.duration
        }
        fn begin_launch(&mut self, _launching: bool) {
            self.began = true;
        }
        fn update_launch(&mut self) {
            self.ticks += 1;
        }
        fn zoom_launch(&self) -> f32 {
            self.zoom
        }
        fn end_launch(&mut self) {
            self.ended = true;
        }
    }

    fn mock() -> Mock {
        Mock {
            duration: 1.0,
            zoom: 2.0,
            ticks: 0,
            began: false,
            ended: false,
        }
    }

    #[test]
    fn landing_progress_runs_zero_to_one() {
        let mut cutscene = Cutscene::new(4.0);
        cutscene.show_landing(Box::new(mock()));
        assert!(cutscene.is_cutscene());
        assert_eq!(cutscene.get_land_time_in(), 0.0);
        cutscene.update(0.0, true);
        assert_eq!(cutscene.get_land_time_in(), 0.0);
        // Advance half a second.
        cutscene.update(30.0, false);
        assert!((cutscene.get_land_time_in() - 0.5).abs() < 1e-3);
        cutscene.update(30.0, false);
        assert!(!cutscene.is_cutscene());
        assert!((cutscene.weather_alpha() - 0.0).abs() < 1e-6);
        // Next update ramps weatherAlpha toward 1.
        cutscene.update(60.0, false);
        assert!(cutscene.weather_alpha() > 0.0);
    }

    #[test]
    fn launching_progress_runs_one_to_zero() {
        let mut cutscene = Cutscene::new(4.0);
        cutscene.show_launch(Box::new(mock()));
        assert!(cutscene.is_launching());
        assert_eq!(cutscene.get_land_time_in(), 1.0);
        cutscene.update(30.0, false);
        assert!((cutscene.get_land_time_in() - 0.5).abs() < 1e-3);
    }

    #[test]
    fn end_launch_is_called() {
        let mut cutscene = Cutscene::new(4.0);
        cutscene.show_landing(Box::new(mock()));
        cutscene.update(70.0, false);
        assert!(!cutscene.is_cutscene());
        assert_eq!(cutscene.land_scale(), 1.0);
    }
}
