use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) struct TestDir(pub PathBuf);

impl TestDir {
    pub fn new(label: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "pecan-{label}-{}-{nonce}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Check actual RIFF length, data length and decoded samples, not just that a
/// writer returned. Float32 WAV uses an extensible header, so find the chunk.
pub(crate) fn assert_wav(path: &Path, channels: u16, rate: u32, samples: &[f32]) {
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(
        u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize,
        bytes.len() - 8
    );
    let data = bytes.windows(4).position(|b| b == b"data").unwrap();
    assert_eq!(
        u32::from_le_bytes(bytes[data + 4..data + 8].try_into().unwrap()) as usize,
        samples.len() * 4
    );
    let mut reader = hound::WavReader::new(std::io::Cursor::new(bytes)).unwrap();
    assert_eq!(
        reader.spec(),
        hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        }
    );
    assert_eq!(
        reader.duration() as usize,
        samples.len() / usize::from(channels)
    );
    assert_eq!(
        reader
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .unwrap(),
        samples
    );
}
