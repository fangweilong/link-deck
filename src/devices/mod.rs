//! LinkDeck 外设硬件通信与电量协议驱动层

pub mod hid_win;       // Windows 底层 HID 原生 API 封装 (CreateFile, WriteFile, ReadFile, Overlapped)
pub mod types;         // 核心数据模型 (DeviceSnapshot, BatteryState, ConnectionMode 等)
pub mod manager;       // 外设聚合管理器 (后台守护线程、热插拔探活、状态快照同步)

// --- 各品牌专属私有协议驱动 ---
pub mod logitech;      // 罗技 (Logitech / Logitech G) HID++ 2.0 / 1.0 (GPW/G502/G304/MX Master/G913)
pub mod razer;         // 雷蛇 (Razer) HyperSpeed 90字节专有报文 (毒蝰/炼狱蝰蛇/巴塞利斯蛇/眼镜蛇)
pub mod steelseries;   // 赛睿 (SteelSeries) 2.4G/有线报文 (Aerox/Prime/Rival/Arctis 耳机/Apex键盘)
pub mod asus_rog;      // 华硕 (ASUS ROG / TUF) 64字节奥创报文 (龙鳞/月刃/战刃/夜魔/魔导士)
pub mod corsair;       // 美商海盗船 (Corsair) Slipstream / Bragi 统一协议 (Dark Core/Katar/K70/K100)
pub mod rk_keyboard;   // RK (Royal Kludge) / Compx 方案三模客制化机械键盘 (毫伏直读)
pub mod gs_keyboard;   // GANSS (迦斯) GS3108T-PRO 三模机械键盘 (0xFF60 命令 0x20 电量)
pub mod saiggi_mouse;  // 赛技 (Saiggi) / Xenta / Areson 双模无线电竞鼠标 (Ping探活与实时捕获)
pub mod mchose;        // 迈从 (MCHOSE) M-HUB 专有协议 (A5/AX5/G3/A7)
pub mod vxe;           // VXE / VGN / ATK / 达尔优 / 造品 / Scyrox 双代 2.4G 协议 (蜻蜓F1/R1/MAD R/Zero)

// --- 通用协议驱动 ---
pub mod bluetooth;     // Windows 原生 BLE GATT Battery Service (0x180F) 全品牌通用蓝牙驱动
pub mod standard_hid;  // USB-IF 官方标准 HID Power Device (0x0084/0x0085) 通用电源驱动

pub use manager::DeviceManager;
pub use types::*;
