/// A small, fast, seedable random number generator (PCG32).
///
/// The same seed always produces the same sequence, which makes
/// gameplay reproducible (replays, debugging, procedural generation).
#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
    increment: u64,
}

const MULTIPLIER: u64 = 6364136223846793005;
const DEFAULT_STREAM: u64 = 0xda3e39cb94b95bdb;

impl Rng {
    pub fn new(seed: u64) -> Self {
        let mut rng = Self {
            state: 0,
            increment: (DEFAULT_STREAM << 1) | 1,
        };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        rng.next_u32();
        rng
    }

    /// Seeded from the system clock: different every run.
    pub fn from_time() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        Self::new(nanos)
    }

    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(MULTIPLIER).wrapping_add(self.increment);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rotation = (old >> 59) as u32;
        xorshifted.rotate_right(rotation)
    }

    /// Uniform in [0, 1).
    pub fn f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * (1.0 / (1u32 << 24) as f32)
    }

    /// Uniform in [min, max).
    pub fn range_f32(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.f32()
    }

    /// Uniform in [min, max). Panics if the range is empty.
    pub fn range_u32(&mut self, min: u32, max: u32) -> u32 {
        assert!(min < max, "empty range {min}..{max}");
        let span = (max - min) as u64;
        min + ((self.next_u32() as u64 * span) >> 32) as u32
    }

    pub fn bool(&mut self) -> bool {
        self.next_u32() & 1 == 1
    }

    /// True with probability `p` (0 to 1).
    pub fn chance(&mut self, p: f32) -> bool {
        self.f32() < p
    }

    /// A random element, or `None` if the slice is empty.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            None
        } else {
            Some(&items[self.range_u32(0, items.len() as u32) as usize])
        }
    }

    /// Shuffle in place (Fisher-Yates).
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.range_u32(0, i as u32 + 1) as usize;
            items.swap(i, j);
        }
    }
}
