//! In-memory WAV for kept-audio playback (parity spec 01 R20): the bytes go
//! straight to the webview, so no plaintext copy ever touches disk.

use crate::stt::SAMPLE_RATE;

/// Sums the two streams sample by sample, clamped to the i16 range (the
/// shorter one counts as silence), and wraps the mix in a 16 kHz mono 16-bit
/// PCM WAV header.
pub fn mixed_wav(mic: &[i16], system: &[i16]) -> Vec<u8> {
    let n = mic.len().max(system.len());
    let data_len = u32::try_from(n * 2).expect("caller caps playback length");
    let mut out = Vec::with_capacity(44 + n * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..n {
        let at = |s: &[i16]| s.get(i).copied().unwrap_or(0);
        out.extend_from_slice(&at(mic).saturating_add(at(system)).to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples(wav: &[u8]) -> Vec<i16> {
        wav[44..]
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect()
    }

    #[test]
    fn as18_header_and_clamped_sum() {
        let wav = mixed_wav(&[1, i16::MAX, i16::MIN, 5], &[2, 10, -10]);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(wav[4..8].try_into().unwrap()), 36 + 8);
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        assert_eq!(u16::from_le_bytes([wav[22], wav[23]]), 1, "mono");
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 16_000);
        assert_eq!(u16::from_le_bytes([wav[34], wav[35]]), 16, "bits");
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 8);
        assert_eq!(samples(&wav), [3, i16::MAX, i16::MIN, 5]);
    }

    #[test]
    fn as18_mic_only_is_the_mic() {
        let wav = mixed_wav(&[7, -7], &[]);
        assert_eq!(wav.len(), 48);
        assert_eq!(samples(&wav), [7, -7]);
    }
}
