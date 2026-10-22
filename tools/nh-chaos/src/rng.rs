//! A small seeded random number generator, so that a failing run can be repeated with its seed.
//!
//! The choices of a run (which pane gets which input, how long a load lasts) come from here. The
//! timing of the processes does not, so a seed makes the choices repeat, not the whole run.

/// SplitMix64. It is not for security.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// A seed from the system, for a run that names none.
    pub fn random_seed() -> u64 {
        let mut bytes = [0u8; 8];
        // A failure leaves the seed 0, which is still a valid seed.
        let _ = getrandom::fill(&mut bytes);
        u64::from_le_bytes(bytes)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `0..n`. `n` must not be 0.
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }

    /// A number in `lo..=hi`.
    pub fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.below(hi - lo + 1)
    }

    /// True in `percent` of 100 calls.
    pub fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[usize::try_from(self.below(items.len() as u64)).unwrap_or(0)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seed_gives_the_same_numbers() {
        let (mut a, mut b) = (Rng::new(7), Rng::new(7));
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        assert_ne!(Rng::new(7).next_u64(), Rng::new(8).next_u64());
    }

    #[test]
    fn ranges_include_both_ends_and_chance_follows_the_percent() {
        let mut rng = Rng::new(1);
        let mut seen = [false; 4];
        for _ in 0..200 {
            seen[usize::try_from(rng.range(2, 5)).expect("small") - 2] = true;
        }
        assert!(seen.iter().all(|s| *s));
        let hits = (0..10_000).filter(|_| rng.chance(30)).count();
        assert!((2_700..3_300).contains(&hits), "{hits}");
        assert!(!rng.chance(0));
    }
}
