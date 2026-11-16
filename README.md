<div align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="112" height="112" alt="FluxScope Logo" />
  <h1>FluxScope</h1>
  <p><strong>面向电机控制（FOC）、高速伺服、数字电源与嵌入式实时测控的原生态高速示波与静态分析平台</strong></p>
  <p><em>A High-Performance Native Oscilloscope, Post-Analysis Workbench &amp; AI-MCP Studio for Motor Control and Embedded Telemetry</em></p>

  <p>
    <a href="https://github.com/ChengJiaAI/FluxScope/releases"><img src="https://img.shields.io/badge/Release-v2.0.0-2563EB?style=flat-square" alt="Release" /></a>
    <img src="https://img.shields.io/badge/Core-Rust%20%2B%20Tauri%20v2-0EA5E9?style=flat-square" alt="Rust + Tauri v2" />
    <img src="https://img.shields.io/badge/Protocols-VOFA%2B%20JustFloat%20%7C%20FireWater%20%7C%20RawData-10B981?style=flat-square" alt="Protocols" />
    <img src="https://img.shields.io/badge/AI%20Native-16%20MCP%20Tools-8B5CF6?style=flat-square" alt="16 MCP Tools" />
    <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-F59E0B?style=flat-square" alt="MIT License" /></a>
  </p>
</div>

---

## 项目简介 | Overview

**FluxScope** 是一款采用 **Rust + Tauri v2 + 原生 HiDPI Canvas2D** 打造的轻量、零外部依赖桌面测控上位机：

- **前端实时观波（`示波`）**：单文件毫秒级冷启动，完整兼容 VOFA+ 三大主流协议（`JustFloat` 小端二进制浮点流、`FireWater` printf 明文流、`RawData` 原始字节流），支持串口（任意非标高波特率如 `468750` / `2000000`）、UDP 与 TCP 双向收发。内置 JustFloat 二进制与 ASCII 文本无损分流器，下位机在高速发波的同时可直接穿插 `printf` 打印电机参数辨识报告。
- **后端静态精析（`分析`）**：将实时流捕获与离线深度算法解耦——提供多维波形对齐与残差量化、多轮调参阶跃整定 PK 表、加窗 FFT/THD 与电谐波阶次成因诊断、离线数字滤波器实时沙盒（直接生成下位机 MCU C 语言代码）、1:1 空间矢量圆包络及全通道统计矩阵。
- **AI 原生双向联调（`16 MCP Tools`）**：单文件直接内置 Stdio MCP（`FluxScope.exe --mcp`）与嵌入式 HTTP JSON-RPC 服务（`http://127.0.0.1:18580/mcp`），无需任何额外插件或 Python 环境，让 Claude Code、Cursor、Antigravity、Codex 等智能体直接读取波形、自动同步通道配置与公式、跑阶跃 PK 算分或设计数字滤波器。

---

## 核心特性 | Key Features

### 1. 高帧率工业级多窗实时示波器
- **微秒级真实时基 & 像素桶包络降采样**：O(log N) 二分视口切片 + Min-Max 双极值包络抽稀，百万采样点缩放拖拽丝滑不丢窄脉冲毛刺；所有实时采集与导出 CSV 均自带微秒级精确 `time_s` 时基列。
- **示波独立显示增益 (`Display Gain`)**：支持对任意通道设置示波器绘波乘数（如将 `0~1` 占空比放大 `100x` 与转速同屏对比），**仅作用于图形高度，绝不污染图例数值、侧栏控件、派生公式与导出数据**。
- **抗飞点自适应 & 二维自由交互**：`自适应` 量程内置离群飞点免疫算法（单帧串口错位乱码不会把正常波形压成直线）；支持画布内滚轮 X+Y 同缩、坐标轴区单轴独立缩放、左键二维平移、VOFA 风格 `10%~100%` 最新帧位置锚点滑块，以及右键直接另存带完整变量颜色图例的高清波形图。

### 2. 上位机实时向量派生公式引擎 (`M0..Mn`)
- 无需重刷固件即可在上位机实时合成新物理量：支持 `CH0..CHn` 或变量简称引用，内置 `sqrt`、`abs`、`sin`、`cos`、`atan2`、`hypot`、`maximum`、`minimum`、`clip`、`lpf(x, alpha)` 一阶低通、`movavg(x, N)` 滑动平均、`rms_win(x, N)` 滑动有效值、`diff(x, dt)` 微分等向量化算子（如实时电压调制比 `sqrt(Vd^2 + Vq^2) / maximum(Vbus / 1.73205, 1.0)`）。

### 3. 可编程模块化右侧仪表矩阵（14 类通用控件 + 多优先级变色）
- 内置 **14 类电控与通用测控可视化控件**：状态枚举徽章、位掩码/多条件状态灯阵（可调列数）、单/双向线性进度条、多通道数值网格、240° 双针圆弧表盘、多路并排立柱图、360° 极坐标双针角度罗盘、实时 X-Y 轨迹图、滚动微趋势统计卡（Sparkline）、曲线快捷显隐面板、SVPWM 空间矢量六边形、Id-Iq 四象限矢量圆、中断分段预算堆栈条、给定-反馈双轨跟随误差条。
- 支持切换右侧栏时间取值源（**最新 / 鼠标参考线 / 游标 T1 / 游标 T2**），并支持配置**多条按优先级排序的条件或自定义表达式超限变色与外框告警**（如 `abs(CH0 - CH1) > 200 || (CH7 & 0x04) != 0`）。

### 4. 六合一后端静态分析工作台（`分析`页）
- **左侧实验数据集池 (`Runs`)**：即开即挂载当前示波缓冲快照，支持一键抓 `T1~T2`、抓全缓冲或直接导入外部 CSV，每轮归档支持单独填写调参备注（如 `Kp=0.8, Ki=120`）。
- **右侧六大功能专页**：
  1. **多维叠图与对齐**：支持「多轮实验同通道 PK」与「单次实验多通道因果叠放」、`0~100%` 归一化、上升沿 50% / 阈值 / 峰值自动归零、`Shift + 左键` 直接拖拽曲线微调 $\Delta t / \Delta y$，以及一键绘制 **$\Delta$ 差值残差曲线**（实时解算 `Max|Δ|` 与 `RMSE`）。
  2. **阶跃整定 PK 表**：自动标定初值/稳态值、上升时间 $t_r$ (`10%→90%`)、峰值时间 $t_p$、超调量 $\sigma\%$、$\pm 2\% / \pm 5\%$ 稳态调节时间 $t_s$、稳态纹波 `P-P` 与跟踪 `RMSE`，多组对比自动高亮绿标 `最优` 项。
  3. **频谱谐波与 THD**：支持 `Hann / Flat-Top / Rectangular` 窗与线性/dB 坐标，自动解算直流偏置、总谐波失真 `THD%`、机械转频 $1\times f_m$ 与电基频 $1\times f_e$，并在谱图上直接标出 `1fe / 2fe / 5fe / 6fe / 7fe` 谐波成因（采样增益不对称、逆变死区非线性、dq 轴六倍频脉动等）。
  4. **离线滤波器沙盒**：内置一阶 IIR 低通、二阶 Butterworth、滑动平均、二阶陷波器（Notch）与零相位双向滤波（FiltFilt），拖动滑块实时预览滤波前后波形对比、纹波衰减率与等效群延迟，并**自动生成可直接复制进固件的 C 语言代码**。
  5. **X-Y 轨迹与外特性**：支持 `1:1` 等比例电压圆 / 电流圆包络分析与最大合成矢量模长统计。
  6. **全通道统计总表**：一键汇总全通道 `Mean / RMS / Min / Max / P-P / StdDev`，支持高清分析图卡 PNG 导出。

### 5. AI-First 原生双向 MCP 直连中心（单文件内置 · 16 个全读写接口）
- 单个 `FluxScope.exe` 同时支持 `FluxScope.exe --mcp`（Stdio 模式，窗口关闭时亦可无头离线读写与静态分析，并支持一键唤醒窗口）及 `http://127.0.0.1:18580/mcp`（窗口在线毫秒级同步），涵盖 `foc_get_status`、`foc_configure_channels`、`foc_upsert_math_channel`、`foc_manage_plots_and_layout`、`foc_post_analysis`、`foc_navigate_and_view`、`foc_capture_screenshot` 等 16 个工具。

---

## 快速上手 | Quick Start

### 方式一：直接下载安装包或绿色版（推荐）
前往右侧 [**Releases**](https://github.com/ChengJiaAI/FluxScope/releases) 页面按习惯任选其一：
- **标准安装版（`FluxScope_2.0.0_x64-setup.exe`）**：双击一键安装，自动生成桌面与开始菜单快捷方式，支持标准卸载。
- **绿色便携单文件版（`FluxScope_2.0.0_x64_Portable.zip`）**：免安装解压即用，仅单个 `FluxScope.exe`（Windows 10 / 11 自带 WebView2 运行时，零环境配置；在同级新建 `portable_data` 空文件夹即可将配置与切片随 U 盘携带）。

### 方式二：从源码编译
环境要求：**Rust (1.78+, MSVC x64)** + Windows SDK / Visual Studio C++ Build Tools。

```powershell
git clone https://github.com/ChengJiaAI/FluxScope.git
cd FluxScope/src-tauri

# 编译单文件独立发行版（同时内置 GUI 与 --mcp 服务）
cargo build --release --bin FluxScope
```
编译产物位于 `src-tauri/target/release/FluxScope.exe`。

---

## 下位机发送参考示例 | MCU Telemetry Snippets

### 1. JustFloat 高速二进制小端协议（高速 FOC 推荐）
```c
// 每帧由连续 N 个 IEEE-754 小端 float 紧跟固定 4 字节帧尾 {0x00, 0x00, 0x80, 0x7F} 组成
// 同时支持在同一串口穿插 printf("Rs=%.4f...\r\n")，FluxScope 会自动无损分流至『终端』页！
typedef struct __attribute__((packed)) {
    float ch[10];
    uint8_t tail[4];
} JustFloatFrame_t;

JustFloatFrame_t tx = { .tail = {0x00, 0x00, 0x80, 0x7F} };
tx.ch[0] = speed_ref;
tx.ch[1] = speed_est;
tx.ch[2] = iq_ref;
tx.ch[3] = iq_fbk;
UART_DMA_Send((uint8_t *)&tx, sizeof(tx));
```

### 2. FireWater 明文逗号协议（零门槛快速验证）
```c
printf("%.2f,%.2f,%.3f,%.2f\n", speed_ref, speed_est, iq_fbk, vbus);
```

---

## 作者与开源许可 | Author & License

- **作者 / Author**：**ChengJia** ([@ChengJiaAI](https://github.com/ChengJiaAI)) — 电控软件算法工程师
- **联系邮箱 / Contact**：`ccdccd999@163.com`
- **问题反馈 / Issues**：欢迎在 [GitHub Issues](https://github.com/ChengJiaAI/FluxScope/issues) 提交 Bug 反馈或新功能建议，也欢迎提交 PR 共建！
- **开源协议 / License**：本项目基于 [MIT License](LICENSE) 开源。
