# LinkDeck - 基于 Rust 的全能外设管理 & 右下角贴边电量 Dock

专为缺少通用管理面板、原厂驱动臃肿且无法直观查看电量的外设打造。完全通过逆向解析原厂协议与集成 Windows 原生蓝牙服务，实现 **底层硬件直连通信**：
- **免安装/免启动官方臃肿软件**（如 RK Keyboard Software、Saiggi TM015，释放数百兆后台内存）。
- **极度轻量极致性能**：单文件仅 ~1.4MB，运行内存仅约 12MB，开机毫秒级启动，零控制台闪烁。
- **全外设通吃**：支持 2.4G 无线接收器、有线模式、蓝牙无线设备（耳机/键鼠/手柄）。
- **高分屏渲染 (High-DPI)**：启用 Windows Per-Monitor V2 原生 DPI 感知与微软雅黑 ClearType 次像素渲染，字迹锐利清晰不虚化，支持自选字体大小（100% / 115% / 130%）。

---

## 🎯 支持的外设协议体系

### 1. RK / Compx 系列机械键盘 (2.4G 无线 / USB 有线)
- **支持品牌/型号**：Royal Kludge 全系（RK108 / R108Pro / S108 / F75 / F99 / M75 / R75 / S98 / RK100 / RK87 / RK84 / RK68 / RK61 等），以及所有采用 Compx 接收器（VID `0x25A7` / `0x3554` / `0x258A`）的客制化三模键盘。
- **协议原理**：
  - 通过 `UsagePage 0xFF02, Usage 0x0002`（COL06 接口）获取 8 字节特性报告：
    $$\text{Request: } [0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]$$
  - 设备返回 8 字节原始数据，提取毫伏高精电压：
    $$\text{voltage\_mv} = \text{buf}[6] \mid (\text{buf}[7] \ll 8) \quad (\text{实测 } 4241\text{ mV})$$
  - 内置 1S 聚合物锂电池放电特征曲线，精准输出 0%~100% 及充电检测（⚡）。

### 2. 赛技 Saiggi / Xenta / Areson 双模无线鼠标
- **支持方案**：赛技 TM015，以及所有采用 Xenta / Areson 主控方案的无线鼠标（VID `0x1D57` / `0x24AE` / `0x093A` 等）。
- **协议原理**：
  - `COL04` 特性端点发送心跳唤醒帧：`[0x0C, 0x0A, 0x01, 0xFE, 0x01, 0xFE, 0, ...]`
  - `COL03`（`UsagePage 0x000A`）异步捕获 Report ID 3 命令 `0x4010`：
    - `buf[3]`：充电状态（1 = 充电中 ⚡，0 = 放电）
    - `buf[4]`：电量档位（1..10，换算 10%~100%）
  - 自动处理无线鼠标深度休眠，鼠标轻触/移动时毫秒级唤醒捕获。

### 3. 罗技 Logitech Lightspeed / Unifying 全系 (2.4G 无线 / USB 有线)
- **支持品牌/型号**：GPW 狗屁王 1/2/3 代、G502 Lightspeed、G304、G703、G903、G913/G915 键盘、MX Master 3/3S 等支持 Lightspeed / Unifying / Bolt 接收器的外设。
- **协议原理**：
  - 基于罗技官方 **HID++ 2.0 / 4.0** 规范（Vendor Collection `0xFF00` / `0xFF43`）。
  - 自动向接收器查询 Root Feature `0x0000` 并解析 `0x1004` (Unified Battery) 与 `0x1000` (Battery Status)。
  - 精确读取当前电量百分比（0%~100%）与充电充能状态。

### 4. 迈从 MCHOSE 全系电竞无线鼠标 (2.4G 无线 / USB 有线)
- **支持品牌/型号**：迈从 A5 / A5 Pro / A5 Pro Max、AX5 / AX5 Pro / AX5 Pro Max、G3 系列、A7 / A7 Ultra、L7、M7 等全系 MCHOSE 鼠标。
- **协议原理**：
  - 基于 M-HUB 专有端点 Feature Report `0x11`（XOR 0xFF 双向掩码校验）。
  - 发送指令 `0x06` 直连读取当前电池百分比、物理充电状态以及当前连接模式（2.4G / 有线 / 蓝牙）。

### 5. VXE / VGN / ATK 全系电竞无线鼠标 (2.4G 无线 / USB 有线)
- **支持品牌/型号**：VXE 蜻蜓 R1 / R1 Pro / R1 SE+、VXE MAD R / Major Plus、VGN 蜻蜓 F1 / F2 系列、ATK F1 / A9 / Zero 系列，以及采用同款 Nordic/Compx 方案的造品 Zaopin Z2、Scyrox V8、达尔优 A950 等。
- **协议原理 (双代协议全面适配)**：
  - **一代 Nordic 52840 (17字节 0x08 0x04 协议)**：覆盖绝大多数主流无线鼠（VXE R1、VGN F1/F2、MAD R、ATK F1 等），自动提取电量百分比、有线状态及电池毫伏值。
  - **二代 Nordic 54L15 (64字节 0x72 0x07 8K协议)**：适配 ATK Zero 等新一代 8K 旗舰无线鼠标及其 8K 专用 Dongle。
### 6. Windows 标准通用 HID 电量外设规范 (Standard HID Power)
- **支持设备**：实现了 Windows 官方标准 HID 电池规范（UsagePage `0x0084` / `0x0085`）的第三方外设。

### 7. Windows 原生蓝牙设备电量服务 (Bluetooth Battery)
- **支持设备**：蓝牙无线耳机、AirPods、Sony、Bose、蓝牙双模鼠标、蓝牙键盘、Xbox 蓝牙手柄等。
- **协议原理**：
  - 通过 Win32 PnP 属性引擎直连 Windows 蓝牙内核栈：
    `DEVPKEY_Device_BatteryLevel` (`{104EA319-6EE2-4701-BD47-8DDBF425BBE5}, 2`)
  - 严谨校验 `DEVPKEY_Device_IsConnected`，严格过滤断开连接的历史残留缓存，杜绝离线幽灵显示。
---

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
