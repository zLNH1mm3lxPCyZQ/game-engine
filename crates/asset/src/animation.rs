use gfx::glam::{Quat, Vec3};

use crate::model::{Model, Pose};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interpolation {
    /// Jump from key to key.
    Step,
    /// Blend smoothly (slerp for rotations).
    Linear,
}

/// One animation being played: which one, where it is, and how it advances.
#[derive(Clone, Copy, Debug)]
struct Track {
    /// Index into `Model::animations`, or `None` for the rest pose.
    animation: Option<usize>,
    time: f32,
    looping: bool,
}

/// Plays animations on one model instance, with crossfades between them.
#[derive(Clone, Debug)]
pub struct Animator {
    current: Track,
    /// The animation fading out, if a crossfade is in progress.
    previous: Option<Track>,
    fade_elapsed: f32,
    fade_duration: f32,
    speed: f32,
    /// Reused pose for sampling the outgoing animation.
    scratch: Option<Pose>,
}

impl Default for Animator {
    fn default() -> Self {
        Self {
            current: Track {
                animation: None,
                time: 0.0,
                looping: true,
            },
            previous: None,
            fade_elapsed: 0.0,
            fade_duration: 0.0,
            speed: 1.0,
            scratch: None,
        }
    }
}

impl Animator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Switch to an animation (`None` = rest pose), crossfading over `fade` seconds.
    /// Playing the animation that's already playing does nothing.
    pub fn play(&mut self, animation: Option<usize>, fade: f32, looping: bool) {
        if self.current.animation == animation {
            return;
        }
        self.previous = (fade > 0.0).then_some(self.current);
        self.current = Track {
            animation,
            time: 0.0,
            looping,
        };
        self.fade_elapsed = 0.0;
        self.fade_duration = fade;
    }

    /// Playback speed multiplier (1 = normal, 0.5 = slow motion).
    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed;
    }

    /// The animation currently playing (or fading in).
    pub fn current(&self) -> Option<usize> {
        self.current.animation
    }

    /// Has a non-looping animation reached its end?
    pub fn is_finished(&self, model: &Model) -> bool {
        match self.current.animation {
            Some(i) if !self.current.looping => self.current.time >= model.animations[i].duration,
            _ => false,
        }
    }

    /// Advance time. Call once per update.
    pub fn update(&mut self, dt: f32) {
        let step = dt * self.speed;
        self.current.time += step;
        if let Some(previous) = &mut self.previous {
            previous.time += step;
            self.fade_elapsed += dt;
            if self.fade_elapsed >= self.fade_duration {
                self.previous = None;
            }
        }
    }

    /// Write the current blended pose, with world transforms updated.
    pub fn apply(&mut self, model: &Model, pose: &mut Pose) {
        sample_track(&self.current, model, pose);

        if let Some(previous) = &self.previous {
            let scratch = self.scratch.get_or_insert_with(|| model.rest_pose());
            sample_track(previous, model, scratch);

            // Fade-in weight of the current animation, eased with smoothstep.
            let t = (self.fade_elapsed / self.fade_duration).clamp(0.0, 1.0);
            let eased = t * t * (3.0 - 2.0 * t);
            // `pose` holds the current animation; pull it back toward the previous one.
            pose.blend(scratch, 1.0 - eased);
        }

        pose.update_worlds(model);
    }
}

/// Reset `pose` to rest and sample one track into it.
fn sample_track(track: &Track, model: &Model, pose: &mut Pose) {
    pose.reset(model);
    let Some(index) = track.animation else { return };
    let animation = &model.animations[index];
    let time = if track.looping && animation.duration > 0.0 {
        track.time % animation.duration
    } else {
        track.time.min(animation.duration)
    };
    animation.sample(time, pose);
}
/// Values over time for one property of one node.
#[derive(Clone, Debug)]
pub enum Keyframes {
    Translation(Vec<Vec3>),
    Rotation(Vec<Quat>),
    Scale(Vec<Vec3>),
}

#[derive(Clone, Debug)]
pub struct Channel {
    /// Index into `Model::nodes`.
    pub node: usize,
    /// Keyframe times in seconds, ascending.
    pub times: Vec<f32>,
    pub keyframes: Keyframes,
    pub interpolation: Interpolation,
}

#[derive(Clone, Debug)]
pub struct Animation {
    pub name: Option<String>,
    /// Time of the last keyframe across all channels, in seconds.
    pub duration: f32,
    pub channels: Vec<Channel>,
}

impl Animation {
    /// Write this animation's values at `time` into `pose`'s local transforms.
    /// Nodes it doesn't animate are left as they are.
    pub fn sample(&self, time: f32, pose: &mut Pose) {
        for channel in &self.channels {
            let (i0, i1, t) = channel.locate(time);
            let local = &mut pose.locals[channel.node];
            match &channel.keyframes {
                Keyframes::Translation(values) => {
                    local.translation = values[i0].lerp(values[i1], t)
                }
                Keyframes::Rotation(values) => local.rotation = values[i0].slerp(values[i1], t),
                Keyframes::Scale(values) => local.scale = values[i0].lerp(values[i1], t),
            }
        }
    }
}

impl Channel {
    /// The two keyframes around `time`, and how far between them it is (0 to 1).
    fn locate(&self, time: f32) -> (usize, usize, f32) {
        let last = self.times.len() - 1;
        if time <= self.times[0] {
            return (0, 0, 0.0);
        }
        if time >= self.times[last] {
            return (last, last, 0.0);
        }
        // The first keyframe strictly after `time`; the one before it is <= time.
        let next = self.times.partition_point(|&k| k <= time);
        let previous = next - 1;
        let t = match self.interpolation {
            Interpolation::Step => 0.0,
            Interpolation::Linear => {
                (time - self.times[previous]) / (self.times[next] - self.times[previous])
            }
        };
        (previous, next, t)
    }
}
