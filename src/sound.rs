//! 低电量危险提示音
//!
//! 电量跌破危险阈值（默认 10%）时与右下角 Toast 同时触发，非阻塞播放内置的
//! “饿死了”语音（`assets/low_battery_esi.wav`，Edge TTS 合成，编译期内嵌）。

use std::ptr;
use windows_sys::Win32::Media::Audio::{
    PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT,
};

/// 编译期内嵌，保证单文件 exe 在没有外部资源时也有声音。
static BUILTIN_ALARM_WAV: &[u8] = include_bytes!("../assets/low_battery_esi.wav");

/// 播放危险电量提示音。`SND_ASYNC` 不阻塞 UI 线程；静态缓冲区在整个进程
/// 生命周期内有效，满足 `SND_MEMORY` 异步播放要求。
pub fn play_low_battery_alarm() {
    unsafe {
        PlaySoundW(
            BUILTIN_ALARM_WAV.as_ptr().cast(),
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let data_pos = BUILTIN_ALARM_WAV
            .windows(4)
            .position(|w| w == b"data")
            .expect("wav data chunk");
        let pcm = &BUILTIN_ALARM_WAV[data_pos + 8..];
        let peak = pcm
            .chunks_exact(2)
            .map(|s| i16::from_le_bytes([s[0], s[1]]).unsigned_abs())
            .max()
            .unwrap_or(0);
        assert!(peak > 3_000, "embedded alarm is silent (peak {peak})");
    }
}
