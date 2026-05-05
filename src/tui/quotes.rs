use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const QUOTES: [&str; 10] = [
    "\"The secret of getting ahead is getting started.\" -- Mark Twain",
    "\"You don't have to be great to start, but you have to start to be great.\" -- Zig Ziglar",
    "\"Amateurs sit and wait for inspiration, the rest of us just get up and go to work.\" -- Stephen King",
    "\"Plans are only good intentions unless they immediately degenerate into hard work.\" -- Peter Drucker",
    "\"Opportunity is missed by most people because it is dressed in overalls and looks like work.\" -- Thomas Edison",
    "\"I'm a greater believer in luck, and I find the harder I work the more I have of it.\" -- Thomas Jefferson",
    "\"There is no traffic jam on the extra mile.\" -- Zig Ziglar",
    "\"It's not that I'm so smart, it's just that I stay with problems longer.\" -- Albert Einstein",
    "\"Your premium brand is what people say about you when you're not in the room.\" -- Jeff Bezos",
    "\"Far and away the best prize that life has to offer is the chance to work hard at work worth doing.\" -- Theodore Roosevelt",
];

pub struct QuoteRotator {
    state: u64,
    index: usize,
    next_change_at: Instant,
}

impl QuoteRotator {
    pub fn new() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15)
            ^ (std::process::id() as u64);
        let mut rotator = Self {
            state: seed.max(1),
            index: 0,
            next_change_at: Instant::now(),
        };
        rotator.rotate();
        rotator
    }

    pub fn refresh_if_due(&mut self) {
        if Instant::now() >= self.next_change_at {
            self.rotate();
        }
    }

    pub fn current_quote(&self) -> &'static str {
        QUOTES[self.index]
    }

    fn rotate(&mut self) {
        let prev = self.index;
        self.index = (self.next_u64() as usize) % QUOTES.len();
        if QUOTES.len() > 1 && self.index == prev {
            self.index = (self.next_u64() as usize) % QUOTES.len();
        }
        let change_after_secs = 3600 + (self.next_u64() % 3601);
        self.next_change_at = Instant::now() + Duration::from_secs(change_after_secs);
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}
