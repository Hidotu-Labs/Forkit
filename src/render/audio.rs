/// Audio engine wrapping SDL2_mixer for `<audio>` element playback.
///
/// Each tab owns one `AudioEngine`.  It tracks the currently-loaded file and
/// exposes simple play/pause/seek controls that map to the player UI.
use sdl2::mixer::{self, Chunk, Channel, InitFlag, DEFAULT_FORMAT};
use sdl2::mixer::LoaderRWops;

use crate::net::resolve_url;

/// We always play on a dedicated channel (channel 0) so there is only ever
/// one audio source playing at a time and we can halt it reliably.
const PLAYBACK_CHANNEL: i32 = 0;
const CHANNELS:         i32 = 8;
const FREQUENCY:        i32 = 44_100;
const CHUNK_SZ:         i32 = 1_024;

/// Initialise SDL2_mixer once per process.  Idempotent — safe to call many
/// times; SDL2_mixer internally ref-counts.
pub fn init_mixer() -> Result<(), String> {
    mixer::open_audio(FREQUENCY, DEFAULT_FORMAT, 2, CHUNK_SZ)
        .map_err(|e| format!("mixer::open_audio: {e}"))?;
    mixer::init(InitFlag::MP3 | InitFlag::OGG | InitFlag::FLAC)
        .map_err(|e| format!("mixer::init: {e}"))?;
    mixer::allocate_channels(CHANNELS);
    Ok(())
}

/// State for a single loaded audio clip.
struct LoadedAudio {
    src:   String,
    chunk: Chunk,
}

/// Per-tab audio engine.
pub struct AudioEngine {
    /// Currently loaded audio, if any.
    loaded:        Option<LoadedAudio>,
    /// Whether playback is active.
    pub playing:   bool,
    /// Cached raw audio bytes (so we don't re-fetch on every frame).
    bytes_cache:   std::collections::HashMap<String, Vec<u8>>,
    /// Simulated position timer (updated each `tick` call).
    pub position_secs:  f64,
    /// Duration in seconds (parsed from file header, or 0 if unknown).
    pub duration_secs:  f64,
    /// Elapsed time at last tick (monotonic, seconds).
    last_tick_instant:  std::time::Instant,
}

impl AudioEngine {
    pub fn new() -> Self {
        AudioEngine {
            loaded:            None,
            playing:           false,
            bytes_cache:       std::collections::HashMap::new(),
            position_secs:     0.0,
            duration_secs:     0.0,
            last_tick_instant: std::time::Instant::now(),
        }
    }

    /// Call once per frame to advance the position timer and detect playback
    /// completion.
    pub fn tick(&mut self) {
        let now = std::time::Instant::now();
        let dt  = (now - self.last_tick_instant).as_secs_f64();
        self.last_tick_instant = now;

        if self.playing {
            self.position_secs += dt;

            // Channel::is_playing() tells us if the mixer is still active.
            let still_going = Channel(PLAYBACK_CHANNEL).is_playing();

            if !still_going {
                // Track finished naturally — reset so the next click restarts.
                self.playing       = false;
                self.position_secs = 0.0;
                // Keep `loaded` so we can replay without re-fetching.
            } else if self.duration_secs > 0.0 {
                self.position_secs = self.position_secs.min(self.duration_secs);
            }
        }
    }

    /// Toggle play/pause for `src`.  If a different src is currently loaded,
    /// stops the old one and loads the new one.
    pub fn toggle(&mut self, src: &str, base_url: &str) {
        if src.is_empty() { return; }
        let resolved = resolve_url(src, base_url);

        if let Some(ref loaded) = self.loaded {
            if loaded.src == resolved {
                // Same track — check real mixer state, not just our flag.
                let mixer_playing = Channel(PLAYBACK_CHANNEL).is_playing();
                if mixer_playing && !self.playing {
                    // This shouldn't happen in normal use, but halt to be safe.
                    let _ = Channel(PLAYBACK_CHANNEL).halt();
                }

                if self.playing {
                    // Currently playing → pause
                    self.pause();
                } else {
                    // Paused or finished → replay from beginning
                    self.replay();
                }
                return;
            }
        }

        // Different (or no) track — stop whatever is playing and load new.
        self.stop();
        self.load_and_play(&resolved, base_url);
    }

    /// Seek to a 0.0–1.0 position within the current track.
    pub fn seek(&mut self, ratio: f64, base_url: &str) {
        if let Some(loaded) = self.loaded.take() {
            let src = loaded.src.clone();
            let dur = self.duration_secs;
            // Halt the channel; chunk drops here.
            let _ = Channel(PLAYBACK_CHANNEL).halt();
            drop(loaded);
            self.playing       = false;
            self.position_secs = ratio * dur;
            self.load_and_play(&src, base_url);
        }
    }

    // ── private helpers ──────────────────────────────────────────────────

    fn pause(&mut self) {
        Channel(PLAYBACK_CHANNEL).pause();
        self.playing = false;
    }

    /// Resume a paused channel.
    fn unpause(&mut self) {
        Channel(PLAYBACK_CHANNEL).resume();
        self.playing = true;
    }

    /// Restart the already-loaded chunk from the beginning.
    fn replay(&mut self) {
        // Halt first to ensure the channel is clean, then play again.
        let _ = Channel(PLAYBACK_CHANNEL).halt();
        if let Some(ref loaded) = self.loaded {
            if let Ok(_ch) = Channel(PLAYBACK_CHANNEL).play(&loaded.chunk, 0) {
                self.playing       = true;
                self.position_secs = 0.0;
            }
        }
    }

    fn stop(&mut self) {
        let _ = Channel(PLAYBACK_CHANNEL).halt();
        self.loaded        = None;
        self.playing       = false;
        self.position_secs = 0.0;
    }

    fn load_and_play(&mut self, resolved: &str, base_url: &str) {
        // Fetch and cache bytes first, then borrow them.
        if !self.bytes_cache.contains_key(resolved) {
            let fetched = self.do_fetch(resolved, base_url);
            match fetched {
                Some(b) => { self.bytes_cache.insert(resolved.to_owned(), b); }
                None    => {
                    eprintln!("[audio] failed to fetch: {resolved}");
                    return;
                }
            }
        }

        // Probe duration while we still have a clean borrow.
        let dur = {
            let b = self.bytes_cache.get(resolved).unwrap();
            probe_duration(b, resolved)
        };

        // Build the SDL Chunk from cached bytes.
        let chunk = {
            let b = self.bytes_cache.get(resolved).unwrap();
            let rwops = match sdl2::rwops::RWops::from_bytes(b) {
                Ok(r)  => r,
                Err(e) => { eprintln!("[audio] RWops error for {resolved}: {e}"); return; }
            };
            match rwops.load_wav() {
                Ok(c)  => c,
                Err(e) => { eprintln!("[audio] decode failed for {resolved}: {e}"); return; }
            }
        };

        // Always halt the dedicated channel before playing on it.
        let _ = Channel(PLAYBACK_CHANNEL).halt();

        match Channel(PLAYBACK_CHANNEL).play(&chunk, 0) {
            Ok(_) => {
                self.playing       = true;
                self.position_secs = 0.0;
                self.duration_secs = dur;
                self.loaded = Some(LoadedAudio { src: resolved.to_owned(), chunk });
            }
            Err(e) => {
                eprintln!("[audio] play failed for {resolved}: {e}");
            }
        }
    }

    /// Fetch bytes from a URL without touching self.bytes_cache (pure I/O).
    fn do_fetch(&self, url: &str, _base_url: &str) -> Option<Vec<u8>> {
        if url.starts_with("file://") {
            let path = url.trim_start_matches("file://");
            std::fs::read(path).ok()
        } else {
            use std::io::Read;
            let resp = ureq::get(url).call().ok()?;
            let mut buf = Vec::new();
            resp.into_reader().read_to_end(&mut buf).ok()?;
            Some(buf)
        }
    }

    /// Fetch the file and probe duration without starting playback.
    /// No-op if bytes are already cached or if `src` is empty.
    pub fn prefetch_duration(&mut self, src: &str, base_url: &str) {
        if src.is_empty() || self.duration_secs > 0.0 { return; }
        if self.bytes_cache.contains_key(src) {
            // Bytes cached but duration not yet probed (shouldn't happen, but handle it)
            let dur = {
                let b = self.bytes_cache.get(src).unwrap();
                probe_duration(b, src)
            };
            self.duration_secs = dur;
            return;
        }
        // Fetch now — this is a blocking call but happens only once per src.
        if let Some(bytes) = self.do_fetch(src, base_url) {
            let dur = probe_duration(&bytes, src);
            self.bytes_cache.insert(src.to_owned(), bytes);
            self.duration_secs = dur;
        }
    }
    pub fn progress(&self) -> f64 {
        if self.duration_secs > 0.0 {
            (self.position_secs / self.duration_secs).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// Src of the currently loaded audio (empty if nothing loaded).
    pub fn current_src(&self) -> &str {
        self.loaded.as_ref().map(|l| l.src.as_str()).unwrap_or("")
    }
}

// ---------------------------------------------------------------------------
// Duration probing — cheap header-only parsing, no full decode needed.
// ---------------------------------------------------------------------------

fn probe_duration(bytes: &[u8], url: &str) -> f64 {
    if bytes.len() < 12 { return 0.0; }

    if &bytes[..4] == b"RIFF" && bytes.len() > 44 && &bytes[8..12] == b"WAVE" {
        return wav_duration(bytes).unwrap_or(0.0);
    }

    if &bytes[..4] == b"fLaC" {
        return flac_duration(bytes).unwrap_or(0.0);
    }

    // OGG: read the last Ogg page to get the granule position.
    if &bytes[..4] == b"OggS" {
        return ogg_duration(bytes).unwrap_or(0.0);
    }

    let ext = url.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    if ext == "mp3" || mp3_sync_word(bytes) {
        return mp3_duration_estimate(bytes).unwrap_or(0.0);
    }

    0.0
}

// ── WAV ──────────────────────────────────────────────────────────────────────

fn wav_duration(bytes: &[u8]) -> Option<f64> {
    let mut pos = 12usize;
    let mut sample_rate     = 0u32;
    let mut num_channels    = 0u16;
    let mut bits_per_sample = 0u16;
    let mut data_bytes      = 0u32;

    while pos + 8 <= bytes.len() {
        let chunk_id   = &bytes[pos..pos + 4];
        let chunk_size = u32::from_le_bytes(bytes[pos+4..pos+8].try_into().ok()?) as usize;
        pos += 8;

        if chunk_id == b"fmt " && chunk_size >= 16 && pos + 16 <= bytes.len() {
            num_channels    = u16::from_le_bytes(bytes[pos+2..pos+4].try_into().ok()?);
            sample_rate     = u32::from_le_bytes(bytes[pos+4..pos+8].try_into().ok()?);
            bits_per_sample = u16::from_le_bytes(bytes[pos+14..pos+16].try_into().ok()?);
        } else if chunk_id == b"data" {
            data_bytes = chunk_size as u32;
        }

        pos += chunk_size + (chunk_size & 1);
        if data_bytes > 0 && sample_rate > 0 { break; }
    }

    if sample_rate == 0 || num_channels == 0 || bits_per_sample == 0 { return None; }
    let bytes_per_sample = (bits_per_sample / 8) as u32;
    let total_samples    = data_bytes / (num_channels as u32 * bytes_per_sample);
    Some(total_samples as f64 / sample_rate as f64)
}

// ── FLAC ─────────────────────────────────────────────────────────────────────

fn flac_duration(bytes: &[u8]) -> Option<f64> {
    if bytes.len() < 42 { return None; }
    let si = &bytes[8..];
    if si.len() < 34 { return None; }
    let sr_raw      = ((si[10] as u32) << 12) | ((si[11] as u32) << 4) | ((si[12] as u32) >> 4);
    let sample_rate = sr_raw & 0xFFFFF;
    let total_hi    = ((si[13] as u64) & 0x0F) << 32;
    let total_lo    = u32::from_be_bytes(si[14..18].try_into().ok()?) as u64;
    let total_samples = total_hi | total_lo;
    if sample_rate == 0 { return None; }
    Some(total_samples as f64 / sample_rate as f64)
}

// ── OGG ──────────────────────────────────────────────────────────────────────

/// Estimate OGG duration by scanning the last ~64 KB for the final Ogg page.
/// The granule position in the last page divided by the sample rate gives duration.
/// We hard-code 44 100 Hz as a fallback; for most Vorbis files this is exact.
fn ogg_duration(bytes: &[u8]) -> Option<f64> {
    // Search backwards for the last "OggS" capture pattern.
    let scan_start = bytes.len().saturating_sub(65536);
    let scan = &bytes[scan_start..];

    let mut last_page_off: Option<usize> = None;
    let mut i = 0usize;
    while i + 4 <= scan.len() {
        if &scan[i..i+4] == b"OggS" {
            last_page_off = Some(i);
        }
        i += 1;
    }

    let off = last_page_off?;
    let page = &scan[off..];
    // Ogg page header: capture(4) + version(1) + hdr_type(1) + granule_pos(8) + ...
    if page.len() < 14 { return None; }
    let granule = u64::from_le_bytes(page[6..14].try_into().ok()?);
    if granule == 0 || granule == u64::MAX { return None; }

    // Try to read sample rate from the first Ogg/Vorbis identification header.
    // It lives in the very first Ogg page body: packet_type(1)+"vorbis"(6)+version(4)+
    // channels(1)+sample_rate(4).
    let sample_rate = vorbis_sample_rate(bytes).unwrap_or(44100);
    Some(granule as f64 / sample_rate as f64)
}

fn vorbis_sample_rate(bytes: &[u8]) -> Option<u32> {
    // Vorbis identification header packet starts with \x01vorbis (7 bytes),
    // then version (4 bytes LE), then channels (1 byte), then sample_rate (4 bytes LE).
    // So sample_rate is at offset +12 from the start of the packet.
    let needle = b"\x01vorbis";
    let limit  = bytes.len().min(65536);
    let mut i  = 0;
    while i + needle.len() + 12 <= limit {
        if &bytes[i..i + needle.len()] == needle {
            let sr_off = i + needle.len() + 4 + 1; // skip version(4) + channels(1)
            if sr_off + 4 <= bytes.len() {
                return Some(u32::from_le_bytes(bytes[sr_off..sr_off + 4].try_into().ok()?));
            }
        }
        i += 1;
    }
    None
}

// ── MP3 ──────────────────────────────────────────────────────────────────────

fn mp3_sync_word(bytes: &[u8]) -> bool {
    let start = id3_skip(bytes);
    let b = &bytes[start..];
    b.len() >= 2 && b[0] == 0xFF && (b[1] & 0xE0) == 0xE0
}

fn id3_skip(bytes: &[u8]) -> usize {
    if bytes.len() >= 10 && &bytes[..3] == b"ID3" {
        let size = ((bytes[6] as usize) << 21)
                 | ((bytes[7] as usize) << 14)
                 | ((bytes[8] as usize) <<  7)
                 |  (bytes[9] as usize);
        10 + size
    } else {
        0
    }
}

fn mp3_duration_estimate(bytes: &[u8]) -> Option<f64> {
    let start = id3_skip(bytes);
    let b = &bytes[start..];
    for i in 0..b.len().saturating_sub(3) {
        if b[i] != 0xFF || (b[i+1] & 0xE0) != 0xE0 { continue; }
        if let Some((bitrate, _sr, _ch, frame_size)) = mp3_frame_params(b, i) {
            if bitrate == 0 || frame_size == 0 { continue; }
            let file_bytes  = bytes.len() as f64;
            let bitrate_bps = bitrate as f64 * 1000.0;
            return Some(file_bytes / (bitrate_bps / 8.0));
        }
    }
    None
}

fn mp3_frame_params(bytes: &[u8], offset: usize) -> Option<(u32, u32, u8, usize)> {
    if offset + 4 > bytes.len() { return None; }
    let b = &bytes[offset..offset + 4];
    let sync = ((b[0] as u32) << 3) | ((b[1] as u32) >> 5);
    if sync != 0x7FF { return None; }
    let version = (b[1] >> 3) & 0x3;
    let layer   = (b[1] >> 1) & 0x3;
    let bit_idx = (b[2] >> 4) & 0xF;
    let sr_idx  = (b[2] >> 2) & 0x3;
    let padding = (b[2] >> 1) & 0x1;
    let channels = (b[3] >> 6) & 0x3;

    if version == 1 || layer == 0 || sr_idx == 3 || bit_idx == 0 || bit_idx == 15 {
        return None;
    }

    let bitrate: u32 = match (version, layer, bit_idx) {
        (3, 1, i) => [0,32,40,48,56,64,80,96,112,128,160,192,224,256,320][i as usize],
        (3, 2, i) => [0,32,48,56,64,80,96,112,128,160,192,224,256,320,384][i as usize],
        (3, 3, i) => [0,32,40,48,56,64,80,96,112,128,160,192,224,256,320][i as usize],
        (2, 3, i) | (0, 3, i) => [0,8,16,24,32,40,48,56,64,80,96,112,128,144,160][i as usize],
        _ => return None,
    };

    let sample_rate: u32 = match (version, sr_idx) {
        (3, 0) => 44100, (3, 1) => 48000, (3, 2) => 32000,
        (2, 0) => 22050, (2, 1) => 24000, (2, 2) => 16000,
        (0, 0) => 11025, (0, 1) => 12000, (0, 2) =>  8000,
        _ => return None,
    };

    let samples_per_frame: u32 = if layer == 3 || layer == 2 { 1152 } else { 384 };
    let frame_size = (samples_per_frame * bitrate * 125 / sample_rate + padding as u32) as usize;
    Some((bitrate, sample_rate, channels, frame_size))
}
