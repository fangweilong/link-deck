# LinkDeck 技术文档

面向二次开发与协议排障。产品功能与使用方式见 [README](../README.md)。

---

## 1. 运行模型

| 组件 | 说明 |
| :--- | :--- |
| 进程 | 单实例常驻，`Local\LinkDeck_SingleInstance_Mutex` 互斥；重复启动只会唤起已有实例的控制中心 |
| 隐藏宿主窗口 | `LinkDeck_HiddenHost`，仅用于接收托盘回调、定时器与跨窗口消息（`WM_COMMAND` / `WM_APP`） |
| 设备轮询线程 | `DeviceManager` 内独立线程，按 `poll_interval_ms` 周期扫描；设置页可随时触发立即刷新 |
| UI 定时器 | 宿主窗口 1s 定时器同步设备快照到悬浮球/面板/托盘；悬浮球自身另有 100ms 动画定时器 |
| 悬浮球窗口 | `LinkDeck_FloatingBallWidget`，`WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW`，色键 + 整体 alpha 双层透明 |
| 控制中心窗口 | `LinkDeck_ManagementPanel`，GDI 双缓冲自绘（内存 DC + `BitBlt`），尺寸/位置记忆在 `config.json` |
| 配置 | `%APPDATA%\LinkDeck\config.json`，serde 反序列化，缺字段自动补默认值 |

### DPI 策略

优先 `DPI_AWARENESS_CONTEXT_UNAWARE_GDISCALED`（由系统按缩放比例放大整幅 GDI 画面，文字仍走 ClearType），失败时回退 `PER_MONITOR_AWARE_V2`。两条路径都使用 `CLEARTYPE_QUALITY` 字体渲染，避免位图拉伸导致的虚化。

### UI 渲染约束

- 设置页由 `settings::layout()` 产出一份带坐标的卡片树，绘制与命中测试读取同一份结果，杜绝“看得见点不着”。
- 设备卡片密度分为紧凑/标准/宽松三档，滚动量按密度换算。
- 悬浮球停靠形态（拉手 `26×64` / 圆球 `48×48`）由用户选择并固定，悬停不再变形，避免两种轮廓差集内反复进出导致闪烁。
- 展开抽屉（232px 宽，字体缩放后 248/268px）不透明度不低于 232/255，保证文字可读。

---

## 2. 设备发现与状态判定

- 设备 ID 采用 `driver + vid + pid` 形式（如 `rk_keyboard_fa70`、`mouse_1d57_fa60`），重命名别名按该 ID 持久化。
- 在线设备才参与悬浮球轮播（`battery.percentage.is_some() || online || charging`）；全部离线时退回显示全部设备。
- 蓝牙设备严格校验 `DEVPKEY_Device_IsConnected`，过滤系统里残留的已断开历史缓存，避免“幽灵电量”。
- 2.4G / 有线设备以 HID 报文回执判断在线；超时或休眠时标记为待机并保留最后一次有效读数。

---

## 3. 协议适配矩阵

| 设备厂家 / 品牌 | 底层主控芯片方案 | 适配协议与代际版本 | 典型支持型号示例 | 连接模式 | 实机验证 |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **罗技 Logitech** | Logitech 原厂无线方案 | **HID++ 2.0 / 4.0** (Unified Battery `0x1004` / Battery Status `0x1000`)<br>**HID++ 1.0** (Register `0x07` 兼容模式)<br>支持 Unifying 优联 1~6 全槽位及 Bolt / Lightspeed 接收器 | **GPW 狗屁王 1/2/3 代**、**G502 Lightspeed**、**G304**、**G903**、**G913/G915 键盘**、**MX Master 3/3S** 全系 | 2.4G 无线 / USB 有线 | ✅ MX Master 3S 已实测 |
| **雷蛇 Razer** | Razer Hyperspeed 无线方案 | **OpenRazer 标准通信体系** (90 字节 Class `0x07` 报文)<br>`CMD 0x80` (电量查询) / `CMD 0x84` (充电指示)，支持 TxID `0x1F` / `0x3F` | **毒蝰 Viper V2 Pro / V3 Pro / Ultimate**、**炼狱蝰蛇 DeathAdder V2/V3 Pro / HyperSpeed**、**巴塞利斯蛇 Basilisk 全系**、**眼镜蛇 Cobra Pro**、**那伽梵蛇**、**八岐大蛇** 等 | 2.4G 无线 / USB 有线 | ⚠️ 未实测 |
| **VXE / VGN / ATK** | Nordic nRF52840<br>Nordic nRF54L15 (8K) | **双代协议全支持**：<br>1. **一代 Nordic 52840 协议** (17 字节 Report `0x08` `0x04`)<br>2. **二代 Nordic 54L15 协议** (64 字节 Report `0x08` `0x72` `0x07` 8K接收器)<br>涵盖 VID `0x373B` / `0x3554` / `0x24AE` | **VXE 蜻蜓 R1 全系** (SE+/Pro/Pro Max)<br>**VXE MAD R / MAD R Major Plus**<br>**VGN 蜻蜓 F1 全系** (F1/Pro/MOBA)<br>**VGN 蜻蜓 F2 系列** (F2 Pro Max/Master+)<br>**ATK F1 / A9 系列**、**ATK Zero 8K 旗舰系列** | 2.4G 无线 / USB 有线 | ✅ VGN 蜻蜓 F1 Pro Max 已实测（其余型号/品牌未验证） |
| **迈从 MCHOSE** | MCHOSE 原厂主控方案 | **双代协议全支持**：<br>1. **一代 Legacy 协议** (Feature Report `0x11` 指令 `0x06`，`XOR 0xFF` 双向掩码)<br>2. **二代 Modern 协议** (Report `0x4D` 64字节 Command `0x0900`)<br>涵盖 VID `0x3837` / `0x5253` / `0x2023` | **迈从 A5 / A5 Pro / A5 Pro Max**、**AX5 / AX5 Pro**、**A5 V3 Pro / A5 V3 Ultra+**、**G3 全系**、**A7 / A7 Ultra / A7 V3**、**L7 / M7 / K7 系列** 等 | 2.4G 无线 / USB 有线 | ⚠️ 未实测 |
| **RK / 狼蛛 / VGN 等键盘** | Compx 键盘方案<br>SinoWealth 中颖方案<br>Sonix 松翰方案 | **Compx 0xFF02 硬件直读协议** (Report ID `0x06` / `0x13`)<br>涵盖 `0x25A7` / `0x3554` / `0x258A` / `0x0C45` / `0x2F24` / `0x320F` / `0x341A` 等全批次 VID | **RK (Royal Kludge) 全系** (RK108 / F75 / F99 / M75 / R75 / S98 / RK100 / RK87 / RK84 / RK68 等)<br>**狼蛛 AULA 全系** (F75 / F87 / F99 / SC680 等接收器 `3554:FA09`)<br>**VGN / VXE 键盘全系** (V87 / V98 / S99 / VXE75 / N75)<br>**新盟 Xinmeng** (X98 / M71 / A66)、**珂芝 KZZI** (K75 / K98)、**瓦尔基里** 等 | 2.4G 无线 / USB 有线 | ✅ RK108 已实测（2.4G `25A7:FA70` / 有线 `258A:00A1`；同方案其他品牌未验证） |
| **赛技 / 黑爵 / 鲨客 / 多彩** | Beken / Xenta / Areson 方案 | **Xenta 异步通信协议** (`UsagePage 0x000A, Report 0x03`)<br>COL04 唤醒探针 + COL03 捕获 `Report 0x03 0x4010`<br>涵盖 `0x1D57` / `0x24AE` / `0x093A` / `0x25A7` | **赛技 Saiggi** (TM015)<br>**黑爵 Ajazz / 鲨客 Attack Shark** (R1 / X3 / X6 / X11 等)<br>**多彩 Delux** (M600 Pro 等) | 2.4G 无线 / USB 有线 | ✅ 赛技 TM015 已实测（2.4G `1D57:FA60` / 有线 `1D57:FA61`；同方案其他品牌未验证） |
| **造品 / Scyrox / 达尔优** | Compx 方案公版鼠标 | **Compx 17 字节协议** (`UsagePage 0xFF02, Report 0x08 0x04`) | **Zaopin 造品 Z2 / Z2 Mini**<br>**Scyrox V8**<br>**达尔优 DAREU A950 / A950 Air / A950 Wing**<br>**G-Wolves Lycan**、**Kysona M600** | 2.4G 无线 / USB 有线 | ⚠️ 未实测 |
| **赛睿 SteelSeries** | SteelSeries 专有无线方案 | **专有报文指令**：`0xD2` / `0x92` 鼠标电量、`0xAA` (Rival 650)、`0x0618` (Arctis 耳机) | **Aerox 3/5/9 Wireless**、**Prime Wireless**、**Rival 650**、**Apex Pro 无线键盘**、**Arctis 7 / 9 / Pro / Nova 系列** | 2.4G 无线 / USB 有线 | ⚠️ 未实测 |
| **华硕 ASUS ROG / TUF** | ROG 原厂主控 + Omni 接收器 | **ROG 专有 64 字节 Feature Report**：`0x1207` 鼠标电量、`0x1201` 键盘电量 | **龙鳞 Ace**、**月刃 Keris**、**战刃 Gladius III**、**夜魔 Azoth**、**魔导士 Falchion**、**Claymore II** | 2.4G 无线 / USB 有线 | ⚠️ 未实测 |
| **美商海盗船 Corsair** | Corsair Slipstream / Bragi + NXP 通道 | **Corsair Bragi 统一协议**：Magic `0x08`，Prop `0x0F` 电量千分比 / `0x10` 充电状态 | **暗影 Dark Core RGB Pro/SE**、**卡特 Katar Pro/Elite**、**铁爪 Ironclaw**、**圣剑 Sabre**、**K70 PRO Mini**、**K100 AIR**、**HS70/80 耳机** | 2.4G 无线 / USB 有线 | ⚠️ 未实测 |
| **客制化键盘 (QMK/VIA)** | QMK / VIAL / 自研固件 | **Windows 原生蓝牙 GATT 规范**<br>Bluetooth SIG 统一 Battery Service (`0x180F`) | **Keychron (渴创)**、**NuPhy**、**WOB 雨75 (Rainy75)**、**桥75 (Bridge75)**、**魔极客 MonsGeek**、**Hi75** 等铝坨坨三模客制化键盘 | 蓝牙无线模式 | ⚠️ 未实测 |
| **全品牌通用蓝牙外设** | 任何蓝牙 4.0+ 芯片 | **Windows 内核级 PnP 蓝牙服务** (`DEVPKEY_Device_BatteryLevel`)<br>结合 `DEVPKEY_Device_IsConnected` 严格剔除已断开的历史死缓存 | 任何品牌的无线蓝牙耳机 (AirPods / 华为 / 索尼 / 漫步者 / Bose 等)、蓝牙鼠标 (小米双模 / 罗技Pebble等)、蓝牙键盘、Xbox 蓝牙无线手柄等 | 蓝牙无线模式 | ✅ P6 耳机已实测（其余设备未逐一验证） |
| **通用标准 HID 规范** | 标准 USB HID 协议 | **Windows HID Power / Battery System 规范**<br>`UsagePage 0x0084` (Power Device) / `0x0085` (Battery System) | 符合微软 Windows 官方标准 HID 电池规范的第三方外设 | 2.4G 无线 / USB 有线 | ⚠️ 未实测 |

> **实机验证状态说明**：✅ 表示已在本机真实硬件上读到并核对电量；⚠️ 未实测表示协议依据公开规范或开源实现编写，尚未用对应硬件验证，可能因固件版本、接收器批次或报告描述符差异失效；⚠️ 待复测表示曾实测失败并已修复，但未收到复测回执。未实测协议不构成可用性承诺。

---

## 4. 各协议探测机制

### 4.1 罗技 Logitech Lightspeed / Unifying 全系

- **协议实现**：基于罗技官方 **HID++ 2.0 / 4.0** 规范（Vendor Collection `0xFF00` / `0xFF43`）。
- **探测机制**：
  - 遍历优联 / Bolt 接收器的全部 **1~6 号配对槽位**（`dev_idx: 1..=6`）以及直接有线设备（`0xFF`）；
  - 构造请求帧第 3 字节携带专用 `HIDPP_SW_ID = 0x05`，接收器百分百正确回执；
  - 先查询 Root Feature `0x0000` 并解析 `0x1004` (Unified Battery)；
  - 若设备不支持 `0x1004`（老款或部分干电池型号），自动 Fallback 至 `0x1000` (Battery Status)，提取当前放电档位百分比；老款设备无缝回退 HID++ 1.0 寄存器 `0x07`；
  - 支持通过 Feature `0x0005` (DeviceName) 直接向硬件固件读取原生设备全名。

### 4.2 雷蛇 Razer Hyperspeed 全系无线电竞鼠标

- **协议实现**：基于 OpenRazer 90 字节 Class `0x07` 标准报文规范。
- **探测机制**：
  - 支持 Transaction ID `0x1F`（新型 8K/4K 鼠标）、`0x3F`（经典无线接收器）以及 `0xFF`；
  - 自动向无线发射端注入 `CMD 0x80`（实时剩余电量）与 `CMD 0x84`（实时充电状态）；
  - 严格执行每帧 90 字节异或校验和（CRC），免装 Razer Synapse 即可直连读取。

### 4.3 VXE / VGN / ATK 全系电竞无线鼠标

- **协议实现**：双代通讯架构全面自适应。
- **探测机制**：
  - **一代 Nordic 52840 (17 字节 Report `0x08 0x04`)**：覆盖 VXE 蜻蜓 R1、VGN 蜻蜓 F1/F2、VXE MAD R、ATK F1、造品 Z2 等，自动解析电量百分比、有线标志及电芯毫伏电压；
  - **二代 Nordic 54L15 (64 字节 Report `0x08 0x72 0x07`)**：适配 ATK Zero 等 8K 旗舰无线鼠标及其 8K 专用发射器；
  - 完整涵盖厂商识别码 `0x373B`、`0x3554`、`0x24AE`、`0x260D`、`0x33E4`。

### 4.4 迈从 MCHOSE 全系电竞无线鼠标

- **协议实现**：双代通讯架构全面自适应。
- **探测机制**：
  - **一代 Legacy 协议**：基于专有端点 Feature Report `0x11`（`0xFF` 双向掩码），注入指令 `0x06` 实时获取电量、充电位和连接模式；
  - **二代 Modern 协议**：基于 Report ID `0x4D` 64 字节报文，注入指令 `0x0900` 兼容全新款 A5 V3 / A7 V3 / G3 V3 / K7 V2 等型号。

### 4.5 RK / 狼蛛 / VGN / 珂芝 / 新盟 三模机械键盘

- **协议实现**：Compx 接收器 `UsagePage 0xFF02, Usage 0x0002` + Report `0x13`。
- **探测机制**：
  - 提取高精电压毫伏读数（`voltage_mv`），内置 1S 聚合物锂电池放电曲线换算为 0%~100%；
  - 全面支持合泰、中颖、松翰及定制主控（VID `0x25A7` / `0x3554` / `0x258A` / `0x0C45` / `0x2F24` / `0x320F` / `0x341A`）。

### 4.6 赛技 / 黑爵 / 鲨客 / 多彩 双模无线鼠标

- **协议实现**：Xenta / Areson 方案 `UsagePage 0x000A, Report 0x03`。
- **探测机制**：COL04 心跳探针唤醒 + COL03 异步解析 `Report 0x03 0x4010`，兼顾休眠节电与毫秒级唤醒捕获。

### 4.7 赛睿 SteelSeries 无线鼠标 / 耳机 / 键盘

- **协议实现**：SteelSeries 专有报文，`0xD2` / `0x92` 查询鼠标电量，`0xAA` 用于 Rival 650，`0x0618` 用于 Arctis 耳机。
- **典型设备**：Aerox 3/5/9 Wireless、Prime Wireless、Rival 650、Apex Pro 无线键盘、Arctis 7 / 9 / Pro / Nova 系列。

### 4.8 华硕 ASUS ROG / TUF 无线电竞外设

- **协议实现**：ROG 专有 64 字节 Feature Report，`0x1207` 读鼠标电量、`0x1201` 读键盘电量，支持 Omni 接收器。
- **典型设备**：龙鳞 Ace (Harpe Ace)、月刃 (Keris)、战刃 (Gladius III)、夜魔 (Azoth)、魔导士 (Falchion)、Claymore II 等。

### 4.9 美商海盗船 Corsair Slipstream / Bragi 无线外设

- **协议实现**：Corsair Bragi 统一协议（Magic `0x08`，Property `0x0F` 电量千分比、`0x10` 充电状态）及 NXP 兼容通道。
- **典型设备**：暗影 Dark Core RGB Pro/SE、卡特 Katar Pro/Elite、铁爪 Ironclaw、圣剑 Sabre、K70 PRO Mini、K100 AIR、HS70/80 耳机等。

### 4.10 Windows 原生蓝牙设备电量服务（全品牌通用）

- **协议实现**：Win32 PnP 属性引擎直连 Windows 蓝牙内核栈 `DEVPKEY_Device_BatteryLevel`；蓝牙 GATT 外设走 Bluetooth SIG 统一 Battery Service `0x180F`，耳机另结合 HFP。
- **防幽灵机制**：严谨校验 `DEVPKEY_Device_IsConnected`，严格过滤断开连接的历史残留缓存，杜绝离线幽灵显示。

---

## 5. 自动更新（单一 exe 自替换）

- 启动 10 秒后自动检查一次 GitHub Releases，此后每 6 小时轮询；可在 `偏好设置 → 系统 → 版本与更新` 关闭。
- 版本比较按语义化版本（`v0.6.0 > v0.5.0`），只有远端更高时才提示，不会重复打扰。
- 升级流程：WinHTTP 下载 → 校验 `MZ` 头与体积 → 把运行中的 exe 改名为 `*.old.exe` → 复制新文件 → 拉起新进程并等待启动标记后旧进程退出 → 下次启动清理备份。
- 新进程被拦截、立即崩溃或 5 秒内未就绪会自动回滚到原版本，并写入 `%APPDATA%\LinkDeck\update.log`；任一步失败原 exe 不会丢失；安装在只读目录（如 `Program Files`）时会给出明确提示。
- 托盘菜单会直接显示「⬆️ 立即升级到 vX.Y.Z」，也可用命令行：

```powershell
link-deck.exe --check-update            # 查询是否有新版本
link-deck.exe --download-update --force # 强制下载当前最新版（用于修复损坏的安装）
link-deck.exe --apply-update <path>     # 用指定 exe 替换自身并重启
```

- 相关实现集中在 `src/update.rs`：Releases JSON 解析优先挑选名为 `link-deck.exe` 的资源，自更新重启通过 `--after-update` 标记与单实例互斥体的等待/释放协调。

---

## 6. 低电量提示音

- 电量跌破危险阈值（默认 `< 10%`）时，右下角 Toast 与语音提示同时触发；同一档位只响一次，充上电后自动复位，重新跌破会再次提醒。
- 内置音效为微软 Edge 神经网络女声合成的「饿死了」，以 WAV 内嵌进 exe（`assets/low_battery_esi.wav`），单文件运行无需任何外部资源；`偏好设置 → 通知 → 低电量 Toast 弹窗` 可开关，点 `🎵 试听` 立即试听。
- 需要重新生成默认音效时执行 `scripts\gen_low_battery_sound.ps1`（依赖 edge-tts 与 ffmpeg），脚本会保留最后一个字的自然收音，不会硬切。

---

## 7. 配置项

配置文件位于 `%APPDATA%\LinkDeck\config.json`（设置页 `系统 → 配置文件目录` 可直接打开）。主要字段：

| 字段 | 含义 |
| :--- | :--- |
| `enable_floating_ball` | 悬浮球开关 |
| `floating_ball_y_ratio` | 悬浮球纵向比例位置 |
| `floating_ball_opacity` | 停靠态不透明度（40–100） |
| `floating_ball_style` | `tab`（贴边拉手）/ `orb`（完整圆球） |
| `poll_interval_ms` | 后台轮询间隔 |
| `font_scale` | 界面字号倍率（1.0 / 1.15 / 1.3） |
| `panel_width` / `panel_height` | 控制中心窗口尺寸记忆 |
| `ui_density` | `compact` / `standard` / `spacious` |
| `device_aliases` | 设备自定义名称，键为设备 ID |
| `enable_battery_toast` / `toast_threshold_*` | 低电量弹窗开关与提示/警告/危险阈值 |
| `toast_gaming_suppression` | 全屏/游戏时免打扰 |
| `enable_low_battery_sound` | 危险阈值语音提示 |
| `device_filter` / `device_sort` | 设备列表筛选与排序 |
| `accent_theme` | 强调色主题 |
| `show_onboarding` | 首次运行引导面板是否显示 |
| `check_updates` | 自动检查更新 |
| `auto_start` | 开机自启 |

---

## 8. 诊断命令

```powershell
link-deck.exe --status     # 已连接外设的电量、电压、协议与物理路径
link-deck.exe --protocols  # 已编译进 exe 的私有协议支持库清单
link-deck.exe --scan-all   # 枚举全部 HID 设备（含未适配型号），用于新增适配
```

`--scan-all` 会打印 UsagePage / Report ID / VID / PID / 物理路径，新增协议适配时以这些原始描述符为输入。
