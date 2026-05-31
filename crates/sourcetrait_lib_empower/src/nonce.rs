use crate::*;

const ALPHABET: &[u8; 62] =
    b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Nonce(u64);

impl Nonce {
    pub fn to_u64(self) -> u64 {
        self.0
    }
}

impl Display for Nonce {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        let mut n = self.0;
        if n == 0 {
            return f.write_str("0");
        }
        let mut buf = [0u8; 11];
        let mut i = 0;
        while n > 0 {
            buf[i] = ALPHABET[(n % 62) as usize];
            n /= 62;
            i += 1;
        }
        buf[..i].reverse();
        let s = std::str::from_utf8(&buf[..i]).expect("ASCII");
        f.write_str(s)
    }
}

pub struct NonceGen {
    counter: AtomicUsize,
}

impl Default for NonceGen {
    fn default() -> Self {
        Self::new()
    }
}

impl NonceGen {
    pub fn new() -> Self {
        Self {
            counter: AtomicUsize::new(0),
        }
    }

    pub fn next<T: Hash>(
        &self,
        payload: &T,
    ) -> Nonce {
        let mut hasher = xxh3::Xxh3::default();
        payload.hash(&mut hasher);
        self.counter
            .fetch_add(1, Ordering::SeqCst)
            .hash(&mut hasher);
        let time_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        time_ns.hash(&mut hasher);
        Nonce(hasher.finish())
    }
}
