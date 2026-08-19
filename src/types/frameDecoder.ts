// ============ 协议输入解析 ============

import type { LogicDecoderConfig } from './logic';

/// 输入格式 — 与 Rust InputFormat 对应 (serde rename_all="lowercase")
export type InputFormat = 'hex' | 'ascii' | 'auto';

// ============ 帧解码控件 (FrameDecoder) ============
//
// 镜像 CommandSender 的块列表设计, 但方向相反:
// - CommandSender: 块列表 → 拼接字节流 → 发送 (输入端口来自 var_ref 块)
// - FrameDecoder:  字节流 → 按块定义解析 → 输出端口 (每个 field/bitfield 块 → 一个输出端口)
//
// 数据来源:
// - 实时模式: 订阅全局 raw_data_collector, 解析传输层接收的字节流
// - 手动测试: 用户在 UI 中粘贴 HEX/ASCII 字符串进行一次性解析
//
// 块类型:
// - header:    匹配帧头固定字节序列 (帧起始标志)
// - length:    读 N 字节为整数, 输出到 length 端口 + 决定后续变长字段长度
// - id:        读 N 字节为整数, 输出到 id_value 端口 + 设置 match_id 上下文
// - field:     按 fieldType 读 N 字节并解码为 f32, 输出到 portName 端口
// - bitfield:  从指定字节按 bit 偏移+位长读取, 输出到 portName 端口
// - checksum:  对前序累计字节校验, 输出 valid 端口 (1.0/0.0)
// - tail:      匹配帧尾固定字节序列 (可选, 帧结束标志)

/// 帧解码块类型
/// 前 7 种为 FrameDecoder 控件 UI 可编辑的块;
/// csv/asciiField/samples 为 schema 模型扩展块 (协议引擎 SchemaEngine 使用,
/// FrameDecoder 控件不提供添加入口)
export type DecoderBlockType =
  | 'header'
  | 'length'
  | 'id'
  | 'field'
  | 'bitfield'
  | 'checksum'
  | 'tail'
  | 'csv'
  | 'asciiField'
  | 'samples';

/// 帧解码校验位置 (与 CommandSender ChecksumPosition 区分, 避免序列化混淆)
/// - append:  校验字节位于帧末尾 (在 tail 之前)
/// - inline:  校验字节位于当前位置 (在块列表中该 checksum 块的位置)
/// - prepend: 校验字节位于帧头之后 (在 header 之后)
export type DecoderChecksumPosition = 'append' | 'inline' | 'prepend';

/// 帧解码块的覆盖范围 (校验计算的字节范围)
/// - all_prior: 从帧开头到本校验块之前的所有字节
/// - range:     用户指定字节偏移范围 [cover_start, cover_end)
export type DecoderChecksumCover = 'all_prior' | 'range';

/// 帧解码块定义 (与 Rust DecoderBlockDef 对应, serde tag="type" content="params")
///
/// 每个块可选 `match_id` 字段 (id 块除外) — 仅当当前帧的 id_value 等于 match_id 时该块执行
/// 未设置 match_id 的块始终执行 (用于多帧类型分派)
export type DecoderBlock =
  | {
      id: string;
      type: 'header';
      label?: string;
      /// 帧头 HEX 字符串 "AA BB"
      hex: string;
      /// 可选 match_id (用于多帧类型分派, 通常 header 不设)
      matchId?: number | null;
    }
  | {
      id: string;
      type: 'length';
      label?: string;
      /// 整数字段类型 (uint8/16/32 LE/BE)
      fieldType: FieldType;
      /// 输出端口名 (默认 "length")
      portName?: string;
      /// 长度单位: bytes=字节数, fields=后续 field 块重复次数
      unit?: 'bytes' | 'fields';
      matchId?: number | null;
    }
  | {
      id: string;
      type: 'id';
      label?: string;
      /// 整数字段类型
      fieldType: FieldType;
      /// 输出端口名 (默认 "id_value")
      portName?: string;
    }
  | {
      id: string;
      type: 'field';
      label?: string;
      /// 字段类型 (int/uint/float LE/BE, bytes)
      fieldType: FieldType;
      /// 输出端口名 (节点上暴露的 Handle id)
      portName: string;
      /// 若设置, 引用某个 length 块的 id — 该字段读取 length_value 字节而非 fieldType 固定长度
      /// (仅 fieldType='bytes' 时生效, 输出第一字节为 f32; 其他类型忽略此字段)
      lengthRef?: string | null;
      /// 仅当 id_value === matchId 时执行 (多帧分派)
      matchId?: number | null;
    }
  | {
      id: string;
      type: 'bitfield';
      label?: string;
      /// 字节偏移 (相对于当前解析位置)
      byteOffset: number;
      /// 位偏移 (0-7)
      bitOffset: number;
      /// 位长度 (1-32)
      bitLength: number;
      /// 是否带符号 (true=最高位为符号位, 二补码)
      isSigned: boolean;
      /// 输出端口名
      portName: string;
      matchId?: number | null;
    }
  | {
      id: string;
      type: 'checksum';
      label?: string;
      /// 校验算法
      algorithm: ChecksumType;
      /// 自定义脚本 (algorithm='custom' 时使用)
      customScript?: string;
      /// 校验覆盖范围
      cover: DecoderChecksumCover;
      /// cover='range' 时的起始字节偏移 (相对帧头之后)
      coverStart?: number;
      /// cover='range' 时的结束字节偏移 ( exclusive)
      coverEnd?: number;
      /// 校验字节在帧中的位置
      position: DecoderChecksumPosition;
      /// 仅当 id_value === matchId 时执行
      matchId?: number | null;
    }
  | {
      id: string;
      type: 'tail';
      label?: string;
      /// 帧尾 HEX 字符串
      hex: string;
      matchId?: number | null;
    }
  // ---- schema 模型扩展块 (Rust DecoderBlockDef 的 Csv/AsciiField/Samples 变体) ----
  // Rust 端这三个变体无 id/match_id 字段; TS 上 id/label 仅作前端 UI 引用 (可选,
  // 序列化后 Rust serde 忽略未知字段, 不影响对齐)
  | {
      /// 分隔符文本帧 (FireWater 类): 一行 = 一帧, 按 separator 切分, 逐列解析为 f32,
      /// 输出到 ports 各端口 (列数多于 ports 时忽略多余列, 少于时缺失端口不输出)
      type: 'csv';
      id?: string;
      label?: string;
      /// 列分隔符 (如 ",")
      separator: string;
      /// 各列输出端口名 (按列序)
      ports: string[];
    }
  | {
      /// ASCII 定宽字段 (Slcan 类): 读 digits 个 ASCII 字符按进制解析为整数, 输出到 portName
      type: 'asciiField';
      id?: string;
      label?: string;
      /// 输出端口名
      portName: string;
      /// 进制 (hex / dec)
      base: 'hex' | 'dec';
      /// 字符数 (定宽)
      digits: number;
    }
  | {
      /// 逻辑解码采样块 (LogicDecode 类): 字节流整体喂入逻辑解码器,
      /// 输出 LogicSample / DecodedEvent 而非 DataFrame 通道
      type: 'samples';
      id?: string;
      label?: string;
      decoder: LogicDecoderConfig;
    };

/// FrameDecoder 控件可编辑的块 — 7 种基础块 (必带 id; 扩展块 csv/asciiField/samples 仅供协议 schema)
export type FrameDecoderBlock = Extract<DecoderBlock, { id: string }>;

/// 帧解码控件配置
export interface FrameDecoderConfig {
  id: string;
  label: string;
  /// 块列表 (按顺序定义帧布局)
  blocks: FrameDecoderBlock[];
  /// 附加输出端口开关
  enableValid: boolean;        // 输出 valid 端口 (1.0=最近帧校验通过, 0.0=失败/未收到)
  enableFrameCount: boolean;   // 输出 frame_count 端口 (累计有效帧数)
  enableLastTimestamp: boolean; // 输出 last_timestamp 端口 (微秒)
  enableFps: boolean;          // 输出 fps 端口 (滑动窗口帧率)
  /// 数据模式: 'live' = 实时数据流, 'manual' = 手动测试
  mode: 'live' | 'manual';
  /// 回环模式: 显示 loopbackIn 字节输入口, 只接收回环边 (CommandSender loopbackOut)
  /// 注入的字节, 不再默认接收实时 RX (与 mode 无关的独立开关)
  loopbackEnabled: boolean;
}

/// 帧解码器手动测试结果 (与 Rust FrameDecoderParseResult 对应)
///
/// 由 parse_frame_decoder_input 命令返回, 用于 manual 模式下的单次解析。
/// - outputs: 端口名 → 值 (来自 field/bitfield/length/id 块 + 可选附加端口)
/// - valid: 校验是否通过
/// - consumedBytes: 本帧消耗的字节数
/// - error: 解析错误信息 (帧不完整/校验失败等)
export interface FrameDecoderManualResult {
  outputs: Record<string, number>;
  valid: boolean;
  consumedBytes: number;
  error?: string;
}

// ============ 命令发送控件 ============

/// 输入格式
/// - hex:         HEX 字节流, 空格分隔 (如 "AA 01 02 BB")
/// - ascii:       ASCII 文本 + 转义字符 (\n \t \r \xHH)
/// - template:    模板字符串 (${VAR} 变量插值)
/// - structured:  结构化字段定义 (按字节序打包)
export type CommandFormat = 'hex' | 'ascii' | 'template' | 'structured';

/// 校验算法类型 (与 lib/checksum.ts 对应)
export type ChecksumType =
  | 'none'
  | 'sum8'
  | 'xor8'
  | 'crc8'
  | 'crc16Modbus'
  | 'crc16CCITT'
  | 'crc32'
  | 'lrc'
  | 'custom';

/// 校验位置
/// - append:  追加到字节流末尾
/// - prepend: 插入到字节流开头
/// - none:    不附加 (仅用于显示)
export type ChecksumPosition = 'append' | 'prepend' | 'none';

/// 结构化字段类型
export type FieldType =
  | 'uint8'
  | 'int8'
  | 'uint16LE'
  | 'uint16BE'
  | 'int16LE'
  | 'int16BE'
  | 'uint32LE'
  | 'uint32BE'
  | 'int32LE'
  | 'int32BE'
  | 'float32LE'
  | 'float32BE'
  | 'bytes';

/// 结构化字段定义
export interface CommandField {
  id: string;
  name: string;
  type: FieldType;
  value: string;  // 字符串形式, 解析时转换
}

/// 数据块类型 — Command 以块列表方式拼接最终字节流
/// - const_hex:   固定字节序列 (如帧头 AA BB)
/// - var_ref:     引用输入端口的值, 按 fieldType 编码 (端口名自定义)
/// - typed_const: 手动输入的字面值, 按 fieldType 编码
/// - checksum:    对前面所有块的累计字节计算校验
export type BlockType = 'const_hex' | 'var_ref' | 'typed_const' | 'checksum';

/// 单个数据块
export interface CommandBlock {
  id: string;
  type: BlockType;
  /// 块显示名 (可选, 用于列表中标识)
  label?: string;
  /// const_hex: hex 字符串 "AA 01 02"
  hex?: string;
  /// var_ref: 自定义输入端口名 (如 "speed"), 节点上按此名暴露 Handle
  portName?: string;
  /// var_ref / typed_const: 数据编码类型
  fieldType?: FieldType;
  /// typed_const: 字面值字符串
  value?: string;
  /// checksum: 校验算法
  checksum?: ChecksumType;
  /// checksum: 自定义校验脚本 (checksum === 'custom' 时使用)
  customScript?: string;
}

/// 命令发送帧 — 一个发送器可携带多个帧, 每个帧独立的块列表与触发方式
export interface CommandFrame {
  id: string;
  label: string;
  /// 数据块列表 (按顺序拼接为最终字节流)
  blocks: CommandBlock[];
  /// 发送后追加 \n
  appendNewline: boolean;
  /// 发送模式 (与回环无关): manual=仅手动, onChange=字节流变化时自动发送, timer=定时自动发送
  sendMode: 'manual' | 'onChange' | 'timer';
  /// 定时发送间隔 ms (sendMode='timer' 有效)
  timerMs: number;
}

/// 命令发送控件配置
/// 以帧列表组织数据块, 每帧独立拼接字节流 / 触发发送; 回环为发送器级
export interface CommandConfig {
  id: string;
  label: string;
  /// 帧列表 (至少一帧)
  frames: CommandFrame[];
  /// 回环模式开关 — 开启后发送的字节会被协议引擎解析并显示对照 (仅影响发送路径, 与发送模式无关)
  loopbackEnabled: boolean;
  /// 回环历史记录
  loopbackHistory: LoopbackResult[];
}

/// 旧版 (单帧) 命令发送控件配置 — 仅用于快照/持久化数据迁移
/// blocks/appendNewline/sendMode/timerMs 已迁入 CommandFrame
export interface LegacyCommandConfig {
  id: string;
  label: string;
  blocks: CommandBlock[];
  appendNewline: boolean;
  loopbackEnabled: boolean;
  sendMode: 'manual' | 'onChange' | 'timer';
  timerMs: number;
  loopbackHistory: LoopbackResult[];
}

/// 协议回环发送-接收结果
export interface LoopbackResult {
  /// 发送的 hex 字符串
  sentHex: string;
  /// 接收到的原始字节
  rxBytes: number[];
  /// 协议引擎解析的帧数
  frameCount: number;
  /// 第一帧的通道值 (非 CAN 协议有效)
  channels: number[];
  /// CAN 帧数
  canCount: number;
}

/// 通用表格显示控件配置
export interface TableViewConfig {
  id: string;
  label: string;
  /// 列定义: 每列对应一个输入端口
  columns: { portName: string; label: string; showRaw?: boolean }[];
  /// 最大保留行数
  maxRows: number;
  /// 是否显示原始字节列
  showRawData: boolean;
  /// 是否显示时间戳列
  showTimestamp: boolean;
}
