//! 低电量提示音
//!
//! 电量跌破危险阈值（默认 10%）时与右下角 Toast 同时触发。四档音色：
//!
//! | 档位 | 来源 | 说明 |
//! |---|---|---|
//! | `Voice` | 内嵌 `assets/low_battery_esi.wav`（Edge TTS） | 原有的“饿死了”语音 |
//! | `Chime` | 运行时合成 | 三音上行和弦，最礼貌 |
//! | `Beep` | 运行时合成 | 两声短促蜂鸣，最直接 |
//! | `Pulse` | 运行时合成 | 低频脉冲，不吵但抓注意力 |
//!
//! 合成音不落盘：首次播放时生成 PCM 并缓存成 `'static` 切片，
//! 满足 `SND_MEMORY` 异步播放对缓冲区生命周期的要求。

use crate::config::AlertSound;
use std::ptr;
use std::sync::OnceLock;
use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};

/// 编译期内嵌，保证单文件 exe 在没有外部资源时也有声音。
static BUILTIN_ALARM_WAV: &[u8] = include_bytes!("../assets/low_battery_esi.wav");

const SAMPLE_RATE: u32 = 44_100;
/// 峰值幅度：留 20% 余量，避免不同声卡上削波
const PEAK: f32 = 0.80;

/// 播放指定音色。`SND_ASYNC` 不阻塞 UI 线程；缓冲区是 `'static` 的，
/// 进程生命周期内始终有效。
pub fn play_alert(kind: AlertSound) {
    let data = wav(kind);
    unsafe {
        PlaySoundW(
            data.as_ptr().cast(),
            ptr::null_mut(),
            SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
        );
    }
}

/// 停止仍在播放的提示音（关闭开关时调用）。
pub fn stop_alarm() {
    unsafe {
        PlaySoundW(ptr::null(), ptr::null_mut(), 0);
    }
}

/// 取某个音色的 WAV 字节。合成音首次调用时生成并缓存。
pub fn wav(kind: AlertSound) -> &'static [u8] {
    static CHIME: OnceLock<Vec<u8>> = OnceLock::new();
    static BEEP: OnceLock<Vec<u8>> = OnceLock::new();
    static PULSE: OnceLock<Vec<u8>> = OnceLock::new();
    match kind {
        AlertSound::Voice => BUILTIN_ALARM_WAV,
        AlertSound::Chime => CHIME.get_or_init(synth_chime).as_slice(),
        AlertSound::Beep => BEEP.get_or_init(synth_beep).as_slice(),
        AlertSound::Pulse => PULSE.get_or_init(synth_pulse).as_slice(),
    }
}

// --------------------------------------------------------------------------
// 合成
// --------------------------------------------------------------------------

/// 单音包络：12ms 起音 + 末段指数收尾，避免“咔哒”声。
fn tone(t: f32, start: f32, dur: f32, freq: f32, amp: f32, bright: f32) -> f32 {
    if t < start || t >= start + dur {
        return 0.0;
    }
    let x = t - start;
    let attack = (x / 0.012).min(1.0);
    let release = ((start + dur - t) / (dur * 0.45).max(0.02)).min(1.0);
    let phase = std::f32::consts::TAU * freq * x;
    let body = phase.sin() + bright * (phase * 2.0).sin();
    body * amp * attack * release
}

/// 把 -1.0..1.0 的采样值包成 16bit 单声道 WAV。
fn wrap_wav(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    let byte_rate = SAMPLE_RATE * 2;

    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());

    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * PEAK * i16::MAX as f32) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

fn render(total: f32, voice: impl Fn(f32) -> f32) -> Vec<u8> {
    let n = (total * SAMPLE_RATE as f32) as usize;
    let samples: Vec<f32> = (0..n).map(|i| voice(i as f32 / SAMPLE_RATE as f32)).collect();
    wrap_wav(&samples)
}

/// 三音上行和弦：C5 → E5 → G5，每音带一点点二次谐波，像一段“叮—咚—叮”。
fn synth_chime() -> Vec<u8> {
    render(0.72, |t| {
        tone(t, 0.00, 0.26, 523.25, 0.55, 0.30)
            + tone(t, 0.13, 0.26, 659.25, 0.55, 0.30)
            + tone(t, 0.26, 0.40, 783.99, 0.60, 0.25)
    })
}

/// 两声短促蜂鸣：880Hz 方波化（叠加奇次谐波），间隔 60ms。
fn synth_beep() -> Vec<u8> {
    render(0.34, |t| {
        let sq = |x: f32| if x.sin() >= 0.0 { 1.0 } else { -1.0 };
        let beep = |start: f32, dur: f32| -> f32 {
            if t < start || t >= start + dur {
                return 0.0;
            }
            let x = t - start;
            let attack = (x / 0.008).min(1.0);
            let release = ((start + dur - t) / 0.01).min(1.0);
            sq(std::f32::consts::TAU * 880.0 * x) * 0.42 * attack * release
        };
        beep(0.0, 0.10) + beep(0.16, 0.10)
    })
}

/// 低频脉冲：196Hz 载波 + 12Hz 颤音，两次起伏，不刺耳但能拽住注意力。
fn synth_pulse() -> Vec<u8> {
    render(0.80, |t| {
        let swell = |start: f32, dur: f32| -> f32 {
            if t < start || t >= start + dur {
                return 0.0;
            }
            let x = t - start;
            let attack = (x / 0.06).min(1.0);
            let release = ((start + dur - t) / 0.12).min(1.0);
            let tremolo = 0.55 + 0.45 * (std::f32::consts::TAU * 9.0 * x).sin();
            let phase = std::f32::consts::TAU * 196.0 * x;
            (phase.sin() + 0.35 * (phase * 2.0).sin()) * attack * release * tremolo * 0.45
        };
        swell(0.0, 0.30) + swell(0.36, 0.40)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peak(wav: &[u8]) -> u16 {
        let data_pos = wav.windows(4).position(|w| w == b"data").expect("wav data chunk");
        wav[data_pos + 8..]
            .chunks_exact(2)
            .map(|s| i16::from_le_bytes([s[0], s[1]]).unsigned_abs())
            .max()
            .unwrap_or(0)
    }

    #[test]
    fn builtin_alarm_is_a_valid_16bit_pcm_wav() {
        assert!(BUILTIN_ALARM_WAV.len() > 44, "embedded wav is empty");
        assert_eq!(&BUILTIN_ALARM_WAV[0..4], b"RIFF");
        assert_eq!(&BUILTIN_ALARM_WAV[8..12], b"WAVE");
        assert_eq!(&BUILTIN_ALARM_WAV[12..16], b"fmt ");
        let channels = u16::from_le_bytes([BUILTIN_ALARM_WAV[22], BUILTIN_ALARM_WAV[23]]);
        let bits = u16::from_le_bytes([BUILTIN_ALARM_WAV[34], BUILTIN_ALARM_WAV[35]]);
        assert_eq!(channels, 1, "mono keeps the exe small");
        assert_eq!(bits, 16);
    }

    #[test]
    fn builtin_alarm_is_not_silent() {
        assert!(peak(BUILTIN_ALARM_WAV) > 3_000, "embedded alarm is silent");
    }

    /// 合成音必须同时满足三件事：合法 WAV、足够响、不削波。
    #[test]
    fn synthesized_tones_are_valid_and_audible() {
        for kind in [AlertSound::Chime, AlertSound::Beep, AlertSound::Pulse] {
            let wav = wav(kind);
            assert_eq!(&wav[0..4], b"RIFF", "{:?} 不是 RIFF", kind);
            assert_eq!(&wav[8..12], b"WAVE", "{:?} 不是 WAVE", kind);
            let declared = u32::from_le_bytes([wav[4], wav[5], wav[6], wav[7]]) as usize + 8;
            assert_eq!(declared, wav.len(), "{:?} 长度字段与实际不符", kind);
            let p = peak(wav);
            assert!(p > 3_000, "{:?} 太轻了 (peak {p})", kind);
            assert!(p < 34_000, "{:?} 削波了 (peak {p})", kind);
        }
    }

    /// 音色之间必须真有区别，否则“换提示音”就是假的。
    #[test]
    fn each_tone_is_distinct() {
        let a = wav(AlertSound::Chime);
        let b = wav(AlertSound::Beep);
        let c = wav(AlertSound::Pulse);
        assert_ne!(a.len(), b.len());
        assert_ne!(b.len(), c.len());
        assert_ne!(a.len(), c.len());
        // 缓存命中：同一音色两次取到的必须是同一块内存
        assert!(std::ptr::eq(wav(AlertSound::Chime), a));
    }

    /// 语音档仍然走内嵌资源，不能被合成音替换掉。
    #[test]
    fn voice_keeps_the_embedded_asset() {
        assert!(std::ptr::eq(wav(AlertSound::Voice), BUILTIN_ALARM_WAV));
    }
}
