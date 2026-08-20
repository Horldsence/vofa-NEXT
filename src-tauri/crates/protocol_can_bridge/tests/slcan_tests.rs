//! SLCAN 协议集成测试

use can_types::{CanDirection, CanFrame};
use protocol_can_bridge::SlcanEngine;
use protocol_engine::{FeedOutput, ProtocolEngine};

/// 解析标准数据帧: t123401020304\r -> id=0x123, dlc=4, data=[0x01,0x02,0x03,0x04]
#[test]
fn test_parse_standard_frame() {
    let mut engine = SlcanEngine::new();
    let frames = engine.feed(b"t123401020304\r").can_frames;
    assert_eq!(frames.len(), 1);
    let f = &frames[0];
    assert_eq!(f.id, 0x123);
    assert!(!f.extended);
    assert!(!f.rtr);
    assert_eq!(f.dlc, 4);
    assert_eq!(f.data, vec![0x01, 0x02, 0x03, 0x04]);
    assert_eq!(f.direction, CanDirection::Rx);
}

/// 解析扩展数据帧: T1234567880102030405060708\r
#[test]
fn test_parse_extended_frame() {
    let mut engine = SlcanEngine::new();
    let frames = engine.feed(b"T1234567880102030405060708\r").can_frames;
    assert_eq!(frames.len(), 1);
    let f = &frames[0];
    assert_eq!(f.id, 0x12345678);
    assert!(f.extended);
    assert!(!f.rtr);
    assert_eq!(f.dlc, 8);
    assert_eq!(f.data, vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
}

/// 解析远程帧: r1234\r
#[test]
fn test_parse_remote_frame() {
    let mut engine = SlcanEngine::new();
    let frames = engine.feed(b"r1234\r").can_frames;
    assert_eq!(frames.len(), 1);
    let f = &frames[0];
    assert_eq!(f.id, 0x123);
    assert!(!f.extended);
    assert!(f.rtr);
    assert_eq!(f.dlc, 4);
    assert!(f.data.is_empty());

    // 扩展远程帧
    let frames = engine.feed(b"R123456784\r").can_frames;
    assert_eq!(frames.len(), 1);
    let f = &frames[0];
    assert_eq!(f.id, 0x12345678);
    assert!(f.extended);
    assert!(f.rtr);
    assert_eq!(f.dlc, 4);
}

/// 分片喂入: 第一次 t1234 不完整, 第二次补齐
#[test]
fn test_parse_partial() {
    let mut engine = SlcanEngine::new();
    let frames = engine.feed(b"t1234").can_frames;
    assert!(frames.is_empty());
    let frames = engine.feed(b"01020304\r").can_frames;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].id, 0x123);
    assert_eq!(frames[0].data, vec![0x01, 0x02, 0x03, 0x04]);
}

/// 忽略非帧命令 (S/O/C/F/V/N 等) 和错误响应
#[test]
fn test_ignore_other_commands() {
    let mut engine = SlcanEngine::new();
    // 设置波特率 + 打开 + 版本 + 序列号 + 错误响应, 均不应产生帧
    let frames = engine.feed(b"S6\rO\rV\rN1234\rz\r").can_frames;
    assert!(frames.is_empty());

    // 混合: 命令 + 数据帧
    let frames = engine.feed(b"S6\rt123401020304\r").can_frames;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].id, 0x123);
}

/// 接受 \n 作为行结束符
#[test]
fn test_accept_newline_terminator() {
    let mut engine = SlcanEngine::new();
    let frames = engine.feed(b"t123401020304\n").can_frames;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].id, 0x123);
}

/// 编码标准数据帧
#[test]
fn test_encode_standard_frame() {
    let mut engine = SlcanEngine::new();
    let frame = CanFrame {
        timestamp: 0,
        id: 0x123,
        extended: false,
        rtr: false,
        dlc: 4,
        data: vec![0x01, 0x02, 0x03, 0x04],
        direction: CanDirection::Tx,
    };
    let encoded = engine.encode_can(&frame);
    assert_eq!(encoded, b"t123401020304\r");
}

/// 编码扩展数据帧
#[test]
fn test_encode_extended_frame() {
    let mut engine = SlcanEngine::new();
    let frame = CanFrame {
        timestamp: 0,
        id: 0x12345678,
        extended: true,
        rtr: false,
        dlc: 8,
        data: vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08],
        direction: CanDirection::Tx,
    };
    let encoded = engine.encode_can(&frame);
    assert_eq!(encoded, b"T1234567880102030405060708\r");
}

/// 编码远程帧
#[test]
fn test_encode_remote_frame() {
    let mut engine = SlcanEngine::new();
    let frame = CanFrame {
        timestamp: 0,
        id: 0x123,
        extended: false,
        rtr: true,
        dlc: 4,
        data: Vec::new(),
        direction: CanDirection::Tx,
    };
    let encoded = engine.encode_can(&frame);
    assert_eq!(encoded, b"r1234\r");

    let frame_ext = CanFrame {
        timestamp: 0,
        id: 0x12345678,
        extended: true,
        rtr: true,
        dlc: 4,
        data: Vec::new(),
        direction: CanDirection::Tx,
    };
    let encoded = engine.encode_can(&frame_ext);
    assert_eq!(encoded, b"R123456784\r");
}

/// 多帧一次性喂入
#[test]
fn test_parse_multiple_frames() {
    let mut engine = SlcanEngine::new();
    let frames = engine.feed(b"t123401020304\rt123401020304\r").can_frames;
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].id, 0x123);
    assert_eq!(frames[1].id, 0x123);
}

/// DLC 为 0 的数据帧
#[test]
fn test_parse_zero_dlc() {
    let mut engine = SlcanEngine::new();
    let frames = engine.feed(b"t1230\r").can_frames;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].dlc, 0);
    assert!(frames[0].data.is_empty());
}

/// 缓冲区溢出保护: 喂入大量无终止符的数据不应崩溃
#[test]
fn test_buffer_overflow_protection() {
    let mut engine = SlcanEngine::new();
    // 喂入 8000 字节无 \r 的数据
    let junk = vec![b'x'; 8000];
    let frames = engine.feed(&junk).can_frames;
    assert!(frames.is_empty());
}

/// 编码标准数据帧 (2 字节): id=0x123, data=[0xAA, 0xBB] → "t1232AABB\r"
#[test]
fn test_encode_standard_frame_2bytes() {
    let mut engine = SlcanEngine::new();
    let frame = CanFrame {
        timestamp: 0,
        id: 0x123,
        extended: false,
        rtr: false,
        dlc: 2,
        data: vec![0xAA, 0xBB],
        direction: CanDirection::Tx,
    };
    let encoded = engine.encode_can(&frame);
    assert_eq!(encoded, b"t1232AABB\r");
}

/// 编码标准远程帧: id=0x100, dlc=0 → "r1000\r"
#[test]
fn test_encode_standard_remote_id_100() {
    let mut engine = SlcanEngine::new();
    let frame = CanFrame {
        timestamp: 0,
        id: 0x100,
        extended: false,
        rtr: true,
        dlc: 0,
        data: Vec::new(),
        direction: CanDirection::Tx,
    };
    let encoded = engine.encode_can(&frame);
    assert_eq!(encoded, b"r1000\r");
}

/// 编码空数据帧 (dlc=0): id=0x123 → "t1230\r"
#[test]
fn test_encode_empty_data_frame() {
    let mut engine = SlcanEngine::new();
    let frame = CanFrame {
        timestamp: 0,
        id: 0x123,
        extended: false,
        rtr: false,
        dlc: 0,
        data: Vec::new(),
        direction: CanDirection::Tx,
    };
    let encoded = engine.encode_can(&frame);
    assert_eq!(encoded, b"t1230\r");
}

/// 编码扩展远程帧: id=0x12345678, dlc=0 → "R123456780\r"
#[test]
fn test_encode_extended_remote_frame_standalone() {
    let mut engine = SlcanEngine::new();
    let frame = CanFrame {
        timestamp: 0,
        id: 0x12345678,
        extended: true,
        rtr: true,
        dlc: 0,
        data: Vec::new(),
        direction: CanDirection::Tx,
    };
    let encoded = engine.encode_can(&frame);
    assert_eq!(encoded, b"R123456780\r");
}

/// 编码 8 字节数据帧 (验证最大数据长度)
#[test]
fn test_encode_max_data_frame() {
    let mut engine = SlcanEngine::new();
    let frame = CanFrame {
        timestamp: 0,
        id: 0x7FF,
        extended: false,
        rtr: false,
        dlc: 8,
        data: vec![0xFF; 8],
        direction: CanDirection::Tx,
    };
    let encoded = engine.encode_can(&frame);
    assert_eq!(encoded, b"t7FF8FFFFFFFFFFFFFFFF\r");
}

/// Round-trip: 标准数据帧 编码后再解析
#[test]
fn test_round_trip_standard_data_frame() {
    let mut engine = SlcanEngine::new();
    let original = CanFrame {
        timestamp: 0,
        id: 0x123,
        extended: false,
        rtr: false,
        dlc: 2,
        data: vec![0xAA, 0xBB],
        direction: CanDirection::Tx,
    };
    let encoded = engine.encode_can(&original);
    let parsed = engine.feed(&encoded).can_frames;
    assert_eq!(parsed.len(), 1);
    let f = &parsed[0];
    assert_eq!(f.id, original.id);
    assert_eq!(f.extended, original.extended);
    assert_eq!(f.rtr, original.rtr);
    assert_eq!(f.dlc, original.dlc);
    assert_eq!(f.data, original.data);
}

/// Round-trip: 扩展数据帧 编码后再解析
#[test]
fn test_round_trip_extended_data_frame() {
    let mut engine = SlcanEngine::new();
    let original = CanFrame {
        timestamp: 0,
        id: 0x12345678,
        extended: true,
        rtr: false,
        dlc: 8,
        data: vec![0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88],
        direction: CanDirection::Tx,
    };
    let encoded = engine.encode_can(&original);
    let parsed = engine.feed(&encoded).can_frames;
    assert_eq!(parsed.len(), 1);
    let f = &parsed[0];
    assert_eq!(f.id, original.id);
    assert_eq!(f.extended, original.extended);
    assert_eq!(f.rtr, original.rtr);
    assert_eq!(f.dlc, original.dlc);
    assert_eq!(f.data, original.data);
}

/// Round-trip: 标准远程帧 编码后再解析
#[test]
fn test_round_trip_standard_remote_frame() {
    let mut engine = SlcanEngine::new();
    let original = CanFrame {
        timestamp: 0,
        id: 0x100,
        extended: false,
        rtr: true,
        dlc: 4,
        data: Vec::new(),
        direction: CanDirection::Tx,
    };
    let encoded = engine.encode_can(&original);
    let parsed = engine.feed(&encoded).can_frames;
    assert_eq!(parsed.len(), 1);
    let f = &parsed[0];
    assert_eq!(f.id, original.id);
    assert_eq!(f.extended, original.extended);
    assert_eq!(f.rtr, original.rtr);
    assert_eq!(f.dlc, original.dlc);
    // 远程帧解析后 data 为空
    assert!(f.data.is_empty());
}

/// Round-trip: 扩展远程帧 编码后再解析
#[test]
fn test_round_trip_extended_remote_frame() {
    let mut engine = SlcanEngine::new();
    let original = CanFrame {
        timestamp: 0,
        id: 0x12345678,
        extended: true,
        rtr: true,
        dlc: 8,
        data: Vec::new(),
        direction: CanDirection::Tx,
    };
    let encoded = engine.encode_can(&original);
    let parsed = engine.feed(&encoded).can_frames;
    assert_eq!(parsed.len(), 1);
    let f = &parsed[0];
    assert_eq!(f.id, original.id);
    assert_eq!(f.extended, original.extended);
    assert_eq!(f.rtr, original.rtr);
    assert_eq!(f.dlc, original.dlc);
    assert!(f.data.is_empty());
}

/// Round-trip: 空数据帧 编码后再解析
#[test]
fn test_round_trip_empty_data_frame() {
    let mut engine = SlcanEngine::new();
    let original = CanFrame {
        timestamp: 0,
        id: 0x55,
        extended: false,
        rtr: false,
        dlc: 0,
        data: Vec::new(),
        direction: CanDirection::Tx,
    };
    let encoded = engine.encode_can(&original);
    let parsed = engine.feed(&encoded).can_frames;
    assert_eq!(parsed.len(), 1);
    let f = &parsed[0];
    assert_eq!(f.id, 0x55);
    assert!(!f.extended);
    assert!(!f.rtr);
    assert_eq!(f.dlc, 0);
    assert!(f.data.is_empty());
}

/// Round-trip: 多帧编码后再解析
#[test]
fn test_round_trip_multiple_frames() {
    let mut engine = SlcanEngine::new();
    let frames = vec![
        CanFrame {
            timestamp: 0,
            id: 0x100,
            extended: false,
            rtr: false,
            dlc: 2,
            data: vec![0xAA, 0xBB],
            direction: CanDirection::Tx,
        },
        CanFrame {
            timestamp: 0,
            id: 0x7FF,
            extended: false,
            rtr: true,
            dlc: 8,
            data: Vec::new(),
            direction: CanDirection::Tx,
        },
    ];
    // 编码两帧后一次性喂入
    let mut buf = Vec::new();
    for f in &frames {
        buf.extend(engine.encode_can(f));
    }
    let parsed = engine.feed(&buf).can_frames;
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].id, 0x100);
    assert_eq!(parsed[0].data, vec![0xAA, 0xBB]);
    assert_eq!(parsed[1].id, 0x7FF);
    assert!(parsed[1].rtr);
}

/// 顺序/并行等价性: 多行 (含非帧命令) + 半行尾
#[test]
fn test_split_aligned_equivalence() {
    let mut data = Vec::new();
    data.extend_from_slice(b"t123401020304\r");
    data.extend_from_slice(b"S6\r"); // 非帧命令
    data.extend_from_slice(b"T000004562AABB\n");
    data.extend_from_slice(b"r1234\r");
    data.extend_from_slice(b"t7893AABB"); // 半行尾 (无终止符)

    // 顺序解析全量
    let mut seq_engine = SlcanEngine::new();
    let seq_frames = seq_engine.feed(&data).can_frames;

    // 并行: split_aligned 切分 + 逐块 new_worker().feed + append 合并
    let ranges = seq_engine
        .split_aligned(&data, 3)
        .expect("slcan 应支持并行切分");
    let tail_start = ranges.last().map(|r| r.end).unwrap_or(0);
    let mut merged = FeedOutput::default();
    let mut concat = Vec::new();
    for r in &ranges {
        let mut w = seq_engine.new_worker();
        merged.append(w.feed(&data[r.clone()]));
        concat.extend_from_slice(&data[r.clone()]);
    }
    concat.extend_from_slice(&data[tail_start..]);

    // concat(块) + tail == 原 data; 半行尾留在 tail 中
    assert_eq!(concat, data);
    assert_eq!(&data[tail_start..], b"t7893AABB");

    // 结果逐项相等 (忽略 timestamp)
    let norm = |f: &CanFrame| (f.id, f.extended, f.rtr, f.dlc, f.data.clone(), f.direction);
    let seq_norm: Vec<_> = seq_frames.iter().map(norm).collect();
    let par_norm: Vec<_> = merged.can_frames.iter().map(norm).collect();
    assert_eq!(seq_norm, par_norm);
    assert_eq!(seq_frames.len(), 3);
}