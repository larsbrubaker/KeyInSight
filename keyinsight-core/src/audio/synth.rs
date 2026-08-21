//! Offline synthesis for the platform audio shells: the metronome click
//! buffers (`Metronome.swift`'s sine bursts) and a rendered-PCM piano
//! playback of the [`MidiFileEncoder`](super::MidiFileEncoder) output.
//!
//! Piano rendering goes through OxiSynth playing the bundled CC0
//! Upright Piano KW soundfont — the exact quality upgrade the Swift
//! architecture doc sanctions ("dropping the CC0 Upright Piano KW SF2
//! into Resources"). A pure additive-synthesis voice remains as the
//! fallback for when the soundfont can't load.
//!
//! Everything here is pure math over `f32` buffers: the native shell
//! feeds the samples to a cpal stream, the WASM shell wraps them in
//! WebAudio `AudioBuffer`s, and tests assert on them directly.

use super::{MidiFileEncoder, RecordedNote};

/// A rendered mono PCM clip.
pub struct Clip {
    pub samples: Vec<f32>,
    pub sample_rate: f64,
}

impl Clip {
    pub fn duration_seconds(&self) -> f64 {
        self.samples.len() as f64 / self.sample_rate
    }
}

/// The metronome click: a 30 ms sine burst with exponential decay —
/// 1000 Hz at 0.5 for beats, 1568 Hz at 0.7 for measure starts (the
/// Swift `makeClick` constants).
pub fn click_samples(sample_rate: f64, accent: bool) -> Vec<f32> {
    let (frequency, amplitude) = if accent { (1568.0, 0.7) } else { (1000.0, 0.5) };
    let frames = (sample_rate * 0.03) as usize;
    (0..frames)
        .map(|i| {
            let t = i as f64 / sample_rate;
            let envelope = (-t * 180.0).exp();
            (amplitude * envelope * (2.0 * std::f64::consts::PI * frequency * t).sin()) as f32
        })
        .collect()
}

/// The bundled piano — Upright Piano KW (FreePats, CC0), small SF2.
pub const SOUNDFONT_BYTES: &[u8] = include_bytes!("../../assets/UprightPianoKW-small.sf2");

/// Render a [`MidiFileEncoder`]-encoded SMF to a mono PCM clip — the
/// soundfont piano when it loads, the synthesized voice otherwise.
/// Returns `None` for data this parser doesn't understand (it reads
/// exactly what the encoder writes: format 0, one track, tempo meta +
/// program change + note on/off).
pub fn render_smf(smf: &[u8], sample_rate: f64) -> Option<Clip> {
    let notes = parse_smf(smf)?;
    Some(
        render_notes_soundfont(&notes, sample_rate)
            .unwrap_or_else(|| render_notes(&notes, sample_rate)),
    )
}

/// Render one immediate note (velocity 80, the Swift `playNote`) to a
/// mono PCM clip — the shells mix it as its own voice so it never
/// disturbs in-flight SMF playback.
pub fn render_note(midi: u8, duration_seconds: f64, sample_rate: f64) -> Option<Clip> {
    let recording = [RecordedNote {
        midi,
        start_seconds: 0.0,
        duration_seconds,
    }];
    render_smf(&MidiFileEncoder::encode_recording(&recording, 0), sample_rate)
}

/// Render through OxiSynth + the bundled SF2. `None` when the soundfont
/// fails to load (the caller falls back to additive synthesis).
fn render_notes_soundfont(notes: &[SmfNote], sample_rate: f64) -> Option<Clip> {
    let mut synth = oxisynth::Synth::new(oxisynth::SynthDescriptor {
        sample_rate: sample_rate as f32,
        reverb_active: false,
        chorus_active: false,
        gain: 0.6,
        ..Default::default()
    })
    .ok()?;
    let font =
        oxisynth::SoundFont::load(&mut std::io::Cursor::new(SOUNDFONT_BYTES)).ok()?;
    synth.add_font(font, true);

    // Flatten note starts/ends into one time-ordered event list.
    enum Edge {
        On(u8, u8),
        Off(u8),
    }
    let mut edges: Vec<(f64, Edge)> = Vec::new();
    for note in notes {
        edges.push((note.start_seconds, Edge::On(note.midi, note.velocity)));
        edges.push((
            note.start_seconds + note.duration_seconds,
            Edge::Off(note.midi),
        ));
    }
    // Offs before ons at the same instant (retrigger the same pitch).
    edges.sort_by(|a, b| {
        a.0.total_cmp(&b.0).then_with(|| {
            let rank = |e: &Edge| matches!(e, Edge::On(..)) as u8;
            rank(&a.1).cmp(&rank(&b.1))
        })
    });

    let end = edges.last().map(|(t, _)| *t).unwrap_or(0.0);
    let frames = ((end + RELEASE_SECONDS) * sample_rate).ceil() as usize;
    let mut left = vec![0.0f32; frames.max(1)];
    let mut right = vec![0.0f32; frames.max(1)];

    let mut cursor = 0usize;
    for (at, edge) in edges {
        let frame = ((at * sample_rate) as usize).min(frames);
        if frame > cursor {
            synth.write((&mut left[cursor..frame], &mut right[cursor..frame]));
            cursor = frame;
        }
        let _ = match edge {
            Edge::On(key, vel) => synth.send_event(oxisynth::MidiEvent::NoteOn {
                channel: 0,
                key,
                vel: vel.min(127),
            }),
            Edge::Off(key) => {
                synth.send_event(oxisynth::MidiEvent::NoteOff { channel: 0, key })
            }
        };
    }
    if frames > cursor {
        synth.write((&mut left[cursor..], &mut right[cursor..]));
    }

    let samples: Vec<f32> = left
        .iter()
        .zip(&right)
        .map(|(l, r)| ((l + r) * 0.5).clamp(-1.0, 1.0))
        .collect();
    Some(Clip {
        samples,
        sample_rate,
    })
}

/// One sounded note from the SMF, in seconds.
#[derive(Debug, Clone, PartialEq)]
pub struct SmfNote {
    pub start_seconds: f64,
    pub duration_seconds: f64,
    pub midi: u8,
    pub velocity: u8,
}

/// Release tail rendered past the last note-off.
const RELEASE_SECONDS: f64 = 0.4;

fn render_notes(notes: &[SmfNote], sample_rate: f64) -> Clip {
    let end = notes
        .iter()
        .map(|n| n.start_seconds + n.duration_seconds)
        .fold(0.0, f64::max);
    let frames = ((end + RELEASE_SECONDS) * sample_rate).ceil() as usize;
    let mut samples = vec![0.0f32; frames.max(1)];

    for note in notes {
        add_piano_voice(&mut samples, sample_rate, note);
    }

    // Soft headroom: normalize only if summed voices exceed it.
    let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if peak > 0.9 {
        let gain = 0.9 / peak;
        for sample in &mut samples {
            *sample *= gain;
        }
    }
    Clip {
        samples,
        sample_rate,
    }
}

/// A decaying-harmonic piano-ish tone: fundamental plus three overtones,
/// 2 ms attack, exponential body decay, fast release after note-off.
fn add_piano_voice(samples: &mut [f32], sample_rate: f64, note: &SmfNote) {
    const HARMONICS: [(f64, f64); 4] = [(1.0, 1.0), (2.0, 0.45), (3.0, 0.22), (4.0, 0.09)];
    let frequency = 440.0 * ((note.midi as f64 - 69.0) / 12.0).exp2();
    let amplitude = note.velocity as f64 / 127.0 * 0.22;
    let start = (note.start_seconds * sample_rate) as usize;
    let voice_len = ((note.duration_seconds + RELEASE_SECONDS) * sample_rate) as usize;

    for i in 0..voice_len {
        let Some(slot) = samples.get_mut(start + i) else {
            break;
        };
        let t = i as f64 / sample_rate;
        // 2 ms attack ramp, then the struck-string body decay.
        let attack = (t / 0.002).min(1.0);
        let body = (-t * 2.2).exp();
        // Fast release once the key lifts.
        let release = if t > note.duration_seconds {
            (-(t - note.duration_seconds) * 18.0).exp()
        } else {
            1.0
        };
        let mut value = 0.0;
        for (mult, gain) in HARMONICS {
            // Higher partials decay faster, like a real string.
            let partial_decay = (-t * 1.5 * (mult - 1.0)).exp();
            value += gain * partial_decay
                * (2.0 * std::f64::consts::PI * frequency * mult * t).sin();
        }
        *slot += (amplitude * attack * body * release * value) as f32;
    }
}

/// Parse the encoder's format-0 SMF into sounded notes (seconds).
pub fn parse_smf(data: &[u8]) -> Option<Vec<SmfNote>> {
    // MThd: magic, length 6, format, tracks, division.
    if data.len() < 22 || &data[0..4] != b"MThd" {
        return None;
    }
    let division = u16::from_be_bytes([data[12], data[13]]) as f64;
    if division == 0.0 || &data[14..18] != b"MTrk" {
        return None;
    }
    let track_len = u32::from_be_bytes([data[18], data[19], data[20], data[21]]) as usize;
    let track = data.get(22..22 + track_len)?;

    let mut notes: Vec<SmfNote> = Vec::new();
    // Note-on times per pitch awaiting their note-off.
    let mut open: Vec<(u8, u8, f64)> = Vec::new(); // (midi, velocity, start_seconds)
    let mut seconds_per_tick = 500_000.0 / 1_000_000.0 / division; // default 120 BPM
    let mut now_seconds = 0.0;
    let mut cursor = 0usize;

    loop {
        let (delta, next) = read_variable_length(track, cursor)?;
        cursor = next;
        now_seconds += delta as f64 * seconds_per_tick;
        let status = *track.get(cursor)?;
        cursor += 1;
        match status {
            0xFF => {
                let meta = *track.get(cursor)?;
                let (len, next) = read_variable_length(track, cursor + 1)?;
                let body = track.get(next..next + len)?;
                cursor = next + len;
                match meta {
                    0x2F => break, // end of track
                    0x51 if len == 3 => {
                        let us_per_quarter =
                            u32::from_be_bytes([0, body[0], body[1], body[2]]) as f64;
                        seconds_per_tick = us_per_quarter / 1_000_000.0 / division;
                    }
                    _ => {}
                }
            }
            0xC0..=0xCF => cursor += 1, // program change
            0x90..=0x9F => {
                let midi = *track.get(cursor)?;
                let velocity = *track.get(cursor + 1)?;
                cursor += 2;
                if velocity > 0 {
                    open.push((midi, velocity, now_seconds));
                } else if let Some(i) = open.iter().position(|(m, _, _)| *m == midi) {
                    let (midi, velocity, start) = open.swap_remove(i);
                    notes.push(note(midi, velocity, start, now_seconds));
                }
            }
            0x80..=0x8F => {
                let midi = *track.get(cursor)?;
                cursor += 2;
                if let Some(i) = open.iter().position(|(m, _, _)| *m == midi) {
                    let (midi, velocity, start) = open.swap_remove(i);
                    notes.push(note(midi, velocity, start, now_seconds));
                }
            }
            _ => return None, // running status / unknown — the encoder never emits it
        }
    }
    // Anything left open sounds to the end of the track.
    for (midi, velocity, start) in open {
        notes.push(note(midi, velocity, start, now_seconds));
    }
    notes.sort_by(|a, b| a.start_seconds.total_cmp(&b.start_seconds));
    Some(notes)
}

fn note(midi: u8, velocity: u8, start: f64, end: f64) -> SmfNote {
    SmfNote {
        start_seconds: start,
        duration_seconds: (end - start).max(0.0),
        midi,
        velocity,
    }
}

/// Read one MIDI variable-length quantity at `at`; returns (value, next).
fn read_variable_length(data: &[u8], at: usize) -> Option<(usize, usize)> {
    let mut value = 0usize;
    let mut cursor = at;
    loop {
        let byte = *data.get(cursor)?;
        cursor += 1;
        value = (value << 7) | (byte & 0x7F) as usize;
        if byte & 0x80 == 0 {
            return Some((value, cursor));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bundled SF2 must actually load and produce sound — a silent
    /// soundfont regression would otherwise hide behind the additive
    /// fallback.
    #[test]
    fn bundled_soundfont_loads_and_sounds() {
        let notes = [SmfNote {
            start_seconds: 0.0,
            duration_seconds: 0.5,
            midi: 60,
            velocity: 80,
        }];
        let clip = render_notes_soundfont(&notes, 44_100.0)
            .expect("bundled Upright Piano KW parses and renders");
        let peak = clip.samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.05, "soundfont piano is audible (peak {peak})");
        assert!(clip.duration_seconds() > 0.5);
    }
}
