# LinkDeck - 基于 Rust 的全能外设管理 & 右下角贴边电量 Dock

专为缺少通用管理面板、原厂驱动臃肿且无法直观查看电量的外设打造。完全通过逆向解析原厂协议与集成 Windows 原生蓝牙服务，实现 **底层硬件直连通信**：
- **免安装/免启动官方臃肿软件**（如 RK Keyboard Software、Saiggi TM015，释放数百兆后台内存）。
- **极度轻量极致性能**：单文件仅 ~1.4MB，运行内存仅约 12MB，开机毫秒级启动，零控制台闪烁。
- **全外设通吃**：支持 2.4G 无线接收器、有线模式、蓝牙无线设备（耳机/键鼠/手柄）。
- **高分屏渲染 (High-DPI)**：启用 Windows Per-Monitor V2 原生 DPI 感知与微软雅黑 ClearType 次像素渲染，字迹锐利清晰不虚化，支持自选字体大小（100% / 115% / 130%）。

---

## 🎯 支持的厂家、方案与协议版本总览

| 设备厂家 / 品牌 | 底层主控芯片方案 | 适配协议与代际版本 | 典型支持型号示例 | 连接模式 |
| :--- | :--- | :--- | :--- | :--- |
| **罗技 Logitech** | Logitech 原厂无线方案 | **HID++ 2.0 / 4.0** (Unified Battery `0x1004` / Battery Status `0x1000`)<br>**HID++ 1.0** (Register `0x07` 兼容模式)<br>支持 Unifying 优联 1~6 全槽位及 Bolt / Lightspeed 接收器 | **GPW 狗屁王 1/2/3 代**、**G502 Lightspeed**、**G304**、**M720 Triathlon**、**G903**、**G913/G915 键盘**、**MX Master 3/3S** 全系 | 2.4G 无线 / USB 有线 |
| **雷蛇 Razer** | Razer Hyperspeed 无线方案 | **OpenRazer 标准通信体系** (90 字节 Class `0x07` 报文)<br>`CMD 0x80` (电量查询) / `CMD 0x84` (充电指示)，支持 TxID `0x1F` / `0x3F` | **毒蝰 Viper V2 Pro / V3 Pro / Ultimate**、**炼狱蝰蛇 DeathAdder V2/V3 Pro / HyperSpeed**、**巴塞利斯蛇 Basilisk 全系**、**眼镜蛇 Cobra Pro**、**那伽梵蛇**、**八岐大蛇** 等 | 2.4G 无线 / USB 有线 |
| **VXE / VGN / ATK** | Nordic nRF52840<br>Nordic nRF54L15 (8K) | **双代协议全支持**：<br>1. **一代 Nordic 52840 协议** (17 字节 Report `0x08` `0x04`)<br>2. **二代 Nordic 54L15 协议** (64 字节 Report `0x08` `0x72` `0x07` 8K接收器)<br>涵盖 VID `0x373B` / `0x3554` / `0x24AE` | **VXE 蜻蜓 R1 全系** (SE+/Pro/Pro Max)<br>**VXE MAD R / MAD R Major Plus**<br>**VGN 蜻蜓 F1 全系** (F1/Pro/MOBA)<br>**VGN 蜻蜓 F2 系列** (F2 Pro Max/Master+)<br>**ATK F1 / A9 系列**、**ATK Zero 8K 旗舰系列** | 2.4G 无线 / USB 有线 |
| **迈从 MCHOSE** | MCHOSE 原厂主控方案 | **双代协议全支持**：<br>1. **一代 Legacy 协议** (Feature Report `0x11` 指令 `0x06`，`XOR 0xFF` 双向掩码)<br>2. **二代 Modern 协议** (Report `0x4D` 64字节 Command `0x0900`)<br>涵盖 VID `0x3837` / `0x5253` / `0x2023` | **迈从 A5 / A5 Pro / A5 Pro Max**、**AX5 / AX5 Pro**、**A5 V3 Pro / A5 V3 Ultra+**、**G3 全系**、**A7 / A7 Ultra / A7 V3**、**L7 / M7 / K7 系列** 等 | 2.4G 无线 / USB 有线 |
| **RK / 狼蛛 / VGN 等键盘** | Compx 键盘方案<br>SinoWealth 中颖方案<br>Sonix 松翰方案 | **Compx 0xFF02 硬件直读协议** (Report ID `0x06` / `0x13`)<br>涵盖 `0x25A7` / `0x3554` / `0x258A` / `0x0C45` / `0x2F24` / `0x320F` / `0x341A` 等全批次 VID | **RK (Royal Kludge) 全系** (RK108 / F75 / F99 / M75 / R75 / S98 / RK100 / RK87 / RK84 / RK68 等)<br>**狼蛛 AULA 全系** (F75 / F87 / F99 / SC680 等接收器 `3554:FA09`)<br>**VGN / VXE 键盘全系** (V87 / V98 / S99 / VXE75 / N75)<br>**新盟 Xinmeng** (X98 / M71 / A66)、**珂芝 KZZI** (K75 / K98)、**瓦尔基里** 等 | 2.4G 无线 / USB 有线 |
| **赛技 / 黑爵 / 鲨客 / 多彩** | Beken / Xenta / Areson 方案 | **Xenta 异步通信协议** (`UsagePage 0x000A, Report 0x03`)<br>COL04 唤醒探针 + COL03 捕获 `Report 0x03 0x4010`<br>涵盖 `0x1D57` / `0x24AE` / `0x093A` / `0x25A7` | **赛技 Saiggi** (TM015)<br>**黑爵 Ajazz / 鲨客 Attack Shark** (R1 / X3 / X6 / X11 等)<br>**多彩 Delux** (M600 Pro 等) | 2.4G 无线 / USB 有线 |
| **造品 / Scyrox / 达尔优** | Compx 方案公版鼠标 | **Compx 17 字节协议** (`UsagePage 0xFF02, Report 0x08 0x04`) | **Zaopin 造品 Z2 / Z2 Mini**<br>**Scyrox V8**<br>**达尔优 DAREU A950 / A950 Air / A950 Wing**<br>**G-Wolves Lycan**、**Kysona M600** | 2.4G 无线 / USB 有线 |
| **客制化键盘 (QMK/VIA)** | QMK / VIAL / 自研固件 | **Windows 原生蓝牙 GATT 规范**<br>Bluetooth SIG 统一 Battery Service (`0x180F`) | **Keychron (渴创)**、**NuPhy**、**WOB 雨75 (Rainy75)**、**桥75 (Bridge75)**、**魔极客 MonsGeek**、**Hi75** 等铝坨坨三模客制化键盘 | 蓝牙无线模式 |
| **全品牌通用蓝牙外设** | 任何蓝牙 4.0+ 芯片 | **Windows 内核级 PnP 蓝牙服务** (`DEVPKEY_Device_BatteryLevel`)<br>结合 `DEVPKEY_Device_IsConnected` 严格剔除已断开的历史死缓存 | 任何品牌的无线蓝牙耳机 (AirPods / 华为 / 索尼 / 漫步者 / Bose 等)、蓝牙鼠标 (小米双模 / 罗技Pebble等)、蓝牙键盘、Xbox 蓝牙无线手柄等 | 蓝牙无线模式 |
| **通用标准 HID 规范** | 标准 USB HID 协议 | **Windows HID Power / Battery System 规范**<br>`UsagePage 0x0084` (Power Device) / `0x0085` (Battery System) | 符合微软 Windows 官方标准 HID 电池规范的第三方外设 | 2.4G 无线 / USB 有线 |

---

## 🔬 核心协议原理与实现细节

### 1. 罗技 Logitech Lightspeed / Unifying 全系 (2.4G 无线 / USB 有线)
- **协议实现**：基于罗技官方 **HID++ 2.0 / 4.0** 规范（Vendor Collection `0xFF00` / `0xFF43`）。
- **探测机制**：
  - 遍历优联 / Bolt 接收器的全部 **1~6 号配对槽位**（`dev_idx: 1..=6`）以及直接有线设备（`0xFF`）；
  - 构造请求帧第 3 字节携带专用 `HIDPP_SW_ID = 0x05`，接收器百分百正确回执；
  - 先查询 Root Feature `0x0000` 并解析 `0x1004` (Unified Battery)；
  - 若设备为干电池型号（如 **M720 Triathlon**），自动 Fallback 至 `0x1000` (Battery Status)，提取当前放电档位百分比；老款设备无缝回退 HID++ 1.0 寄存器 `0x07`；
  - 支持通过 Feature `0x0005` (DeviceName) 直接向硬件固件读取原生设备全名。

### 2. 雷蛇 Razer Hyperspeed 全系无线电竞鼠标 (2.4G 无线 / USB 有线)
- **协议实现**：基于 OpenRazer 90 字节 Class `0x07` 标准报文规范。
- **探测机制**：
  - 支持 Transaction ID `0x1F`（新型 8K/4K 鼠标）、`0x3F`（经典无线接收器）以及 `0xFF`；
  - 自动向无线发射端注入 `CMD 0x80`（实时剩余电量）与 `CMD 0x84`（实时充电状态）；
  - 严格执行每帧 90 字节异或校验和（CRC），免装 Razer Synapse 即可直连读取。

### 3. VXE / VGN / ATK 全系电竞无线鼠标 (2.4G 无线 / USB 有线)
- **协议实现**：双代通讯架构全面自适应。
- **探测机制**：
  - **一代 Nordic 52840 (17 字节 Report `0x08 0x04`)**：覆盖 VXE 蜻蜓 R1、VGN 蜻蜓 F1/F2、VXE MAD R、ATK F1、造品 Z2 等，自动解析电量百分比、有线标志及电芯毫伏电压；
  - **二代 Nordic 54L15 (64 字节 Report `0x08 0x72 0x07`)**：适配 ATK Zero 等 8K 旗舰无线鼠标及其 8K 专用发射器；
  - 完整涵盖厂商识别码 `0x373B`、`0x3554`、`0x24AE`、`0x260D`、`0x33E4`。

### 4. 迈从 MCHOSE 全系电竞无线鼠标 (2.4G 无线 / USB 有线)
- **协议实现**：双代通讯架构全面自适应。
- **探测机制**：
  - **一代 Legacy 协议**：基于专有端点 Feature Report `0x11`（`0xFF` 双向掩码），注入指令 `0x06` 实时获取电量、充电位和连接模式；
  - **二代 Modern 协议**：基于 Report ID `0x4D` 64 字节报文，注入指令 `0x0900` 兼容全新款 A5 V3 / A7 V3 / G3 V3 / K7 V2 等型号。

### 5. RK / 狼蛛 / VGN / 珂芝 / 新盟 三模机械键盘 (2.4G 无线 / USB 有线)
- **协议实现**：Compx 接收器 `UsagePage 0xFF02, Usage 0x0002` + Report `0x13`。
- **探测机制**：
  - 提取高精电压毫伏读数（`voltage_mv`），内置 1S 聚合物锂电池放电曲线换算为 0%~100%；
  - 全面支持合泰、中颖、松翰及定制主控（VID `0x25A7` / `0x3554` / `0x258A` / `0x0C45` / `0x2F24` / `0x320F` / `0x341A`）。

### 6. 赛技 / 黑爵 / 鲨客 / 多彩 双模无线鼠标 (2.4G 无线 / USB 有线)
- **协议实现**：Xenta / Areson 方案 `UsagePage 0x000A, Report 0x03`。
- **探测机制**：COL04 心跳探针唤醒 + COL03 异步解析 `Report 0x03 0x4010`，兼顾休眠节电与毫秒级唤醒捕获。

### 7. Windows 原生蓝牙设备电量服务 (全品牌通用)
- **协议实现**：Win32 PnP 属性引擎直连 Windows 蓝牙内核栈 `DEVPKEY_Device_BatteryLevel`。
- **防幽灵机制**：严谨校验 `DEVPKEY_Device_IsConnected`，严格过滤断开连接的历史残留缓存，杜绝离线幽灵显示。

## 💻 界面交互设计

1. **右下角贴边折叠悬浮球 (Edge-sticking Floating Ball)**：
   - 默认开启，贴附于屏幕右下边缘（任务栏上方）：
     - **收起状态（能量环与智能轮播）**：小巧圆球（44×44px），采用 360° 次像素超采样抗锯齿**动态能量环 (Energy Ring)**：
       - **电量环弧度与颜色**：高电量绿环 ($\ge 50\%$)、中电量橙环 ($20\% \sim 49\%$)、低电量红环 ($< 20\%$)。
       - **插线/充电状态自动感知**：插线充能时自动切换为**电光青蓝脉冲呼吸光环**，顶部常亮金色/青色 `⚡` 闪电角标。
       - **多外设自动轮播 (Carousel)**：底部带有微型多点指示器（Pagination Dots），每 3 秒自动平滑轮播当前活跃外设（键盘 ⌨️ / 鼠标 🖱️ / 蓝牙耳机 🎧）；鼠标悬停在悬浮球上**滚动鼠标滚轮**可立即手动切台。
     - **展开状态**：点击小球向左滑出抽屉式外设卡片预览，包含各设备电量进度条、供电来源（电脑USB充电 / 充电头充电）与毫伏高精度电压，支持点击右上角 `▶` 快速收回。
   - **随手拖拽**：长按鼠标左键可沿屏幕边缘自由上下拖拽，松手自动贴边吸附并记忆高度。
2. **任务栏系统托盘 (System Tray)**：
   - 悬停即显详尽电量气泡，右键提供完整控制菜单（包括悬浮球开关、字号切换等）。
3. **外设控制中心主面板 (Control Center Dashboard)**：
   - 点击小球底部的 `⚙️ 打开控制中心` 或托盘菜单即可唤起。
   - 包含多设备卡片、高精度毫伏电压读数、协议类型标注，以及内置 **字体大小切换器（100% 标准 / 115% 适中 / 130% 放大）**。

---

## 🛠️ 构建与运行

### 命令行常用指令
```powershell
# 1. 查看当前已扫描并连接的外设电量状态
link-deck.exe --status

# 2. 查看已支持的私有协议与硬件支持库清单
link-deck.exe --protocols

# 3. 诊断探测当前电脑插入的所有底层 HID 设备 (用于未识别设备适配)
link-deck.exe --scan-all
```
### 启动常驻程序
```powershell
cargo run --release
# 或直接双击运行 target\release\link-deck.exe
```
