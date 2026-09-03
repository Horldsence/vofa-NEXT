//! candleLight 协议集成测试

use can_types::{CanDirection, CanFrame};
use protocol_can_bridge::{
    CandleEngine, CAND_CMD_RX, CAND_CMD_TX, CAND_FRAME_SIZE, CAND_ID_EFF, CAND_ID_MASK, CAND_ID_RTR,
};
use protocol_engine::{FeedOutput, ProtocolEngine};

/// 构造一个 24 字节帧
fn make_rx_frame(cmd_id: u8, can_id_raw: u32, dlc: u8, data: &[u8]) -> Vec<u8> {
    let mut pkt = vec![0u8; CAND_FRAME_SIZE];
    pkt[0] = cmd_id;
    pkt[8..12].copy_from_slice(&can_id_raw.to_le_bytes());
    pkt[12] = dlc & 0x0F;
    for (i, &b) in data.iter().enumerate().take(8) {
        pkt[16 + i] = b;
    }
    pkt
}

#[test]
fn test_parse_rx_frame() {
    let mut engine = CandleEngine::new();
    let pkt = make_rx_frame(CAND_CMD_RX, 0x123, 4, &[0x01, 0x02, 0x03, 0x04]);
    let frames = engine.feed(&pkt).can_frames;
    assert_eq!(frames.len(), 1);
    let f = &frames[0];
    assert_eq!(f.id, 0x123);
    assert!(!f.extended);
    assert!(!f.rtr);
    assert_eq!(f.dlc, 4);
    assert_eq!(f.data, vec![0x01, 0x02, 0x03, 0x04]);
    assert_eq!(f.direction, CanDirection::Rx);
}

#[test]
fn test_parse_extended_frame() {
    let mut engine = CandleEngine::new();
    let can_id_raw = 0x12345678 | CAND_ID_EFF;
    let pkt = make_rx_frame(
        CAND_CMD_RX,
        can_id_raw,
        8,
        &[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08],
    );
    let frames = engine.feed(&pkt).can_frames;
    assert_eq!(frames.len(), 1);
    let f = &frames[0];
    assert_eq!(f.id, 0x12345678);
    assert!(f.extended);
    assert!(!f.rtr);
    assert_eq!(f.dlc, 8);
    assert_eq!(f.data, vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
}

#[test]
fn test_parse_rtr_frame() {
    let mut engine = CandleEngine::new();
    let can_id_raw = 0x123 | CAND_ID_RTR;
    let pkt = make_rx_frame(CAND_CMD_RX, can_id_raw, 4, &[]);
    let frames = engine.feed(&pkt).can_frames;
    assert_eq!(frames.len(), 1);
    let f = &frames[0];
    assert_eq!(f.id, 0x123);
    assert!(!f.extended);
    assert!(f.rtr);
    assert_eq!(f.dlc, 4);
}

#[test]
fn test_parse_partial() {
    let mut engine = CandleEngine::new();
    let pkt = make_rx_frame(CAND_CMD_RX, 0x123, 4, &[0x01, 0x02, 0x03, 0x04]);
    let frames = engine.feed(&pkt[..12]).can_frames;
    assert!(frames.is_empty());
    let frames = engine.feed(&pkt[12..]).can_frames;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].id, 0x123);
    assert_eq!(frames[0].data, vec![0x01, 0x02, 0x03, 0x04]);
}

#[test]
fn test_skip_non_frame_command() {
    let mut engine = CandleEngine::new();
    let mut pkt = vec![0u8; CAND_FRAME_SIZE];
    pkt[0] = 0x01;
    let frames = engine.feed(&pkt).can_frames;
    assert!(frames.is_empty());
    let valid_pkt = make_rx_frame(CAND_CMD_RX, 0x123, 4, &[0x01, 0x02, 0x03, 0x04]);
    let frames = engine.feed(&valid_pkt).can_frames;
    assert_eq!(frames.len(), 1);
}

#[test]
fn test_parse_tx_frame() {
    let mut engine = CandleEngine::new();
    let pkt = make_rx_frame(CAND_CMD_TX, 0x123, 2, &[0xAA, 0xBB]);
    let frames = engine.feed(&pkt).can_frames;
    assert_eq!(frames.len(), 1);
    let f = &frames[0];
    assert_eq!(f.direction, CanDirection::Tx);
    assert_eq!(f.data, vec![0xAA, 0xBB]);
}

#[test]
fn test_encode_tx_frame() {
    let mut engine = CandleEngine::new();
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
    assert_eq!(encoded.len(), CAND_FRAME_SIZE);
    assert_eq!(encoded[0], CAND_CMD_TX);
    let can_id_raw = u32::from_le_bytes([encoded[8], encoded[9], encoded[10], encoded[11]]);
    assert_eq!(can_id_raw, 0x123);
    assert_eq!(encoded[12], 4);
    assert_eq!(&encoded[16..20], &[0x01, 0x02, 0x03, 0x04]);
}

#[test]
fn test_encode_extended_rtr_frame() {
    let mut engine = CandleEngine::new();
    let frame = CanFrame {
        timestamp: 0,
        id: 0x12345678,
        extended: true,
        rtr: true,
        dlc: 4,
        data: Vec::new(),
        direction: CanDirection::Tx,
    };
    let encoded = engine.encode_can(&frame);
    assert_eq!(encoded.len(), CAND_FRAME_SIZE);
    let can_id_raw = u32::from_le_bytes([encoded[8], encoded[9], encoded[10], encoded[11]]);
    assert_eq!(can_id_raw & CAND_ID_MASK, 0x12345678);
    assert!(can_id_raw & CAND_ID_EFF != 0);
    assert!(can_id_raw & CAND_ID_RTR != 0);
}

#[test]
fn test_parse_multiple_frames() {
    let mut engine = CandleEngine::new();
    let mut data = Vec::new();
    data.extend_from_slice(&make_rx_frame(CAND_CMD_RX, 0x100, 1, &[0xAA]));
    data.extend_from_slice(&make_rx_frame(CAND_CMD_RX, 0x200, 1, &[0xBB]));
    let frames = engine.feed(&data).can_frames;
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].id, 0x100);
    assert_eq!(frames[1].id, 0x200);
}

#[test]
fn test_encode_standard_frame_full_structure() {
    let mut engine = CandleEngine::new();
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
    assert_eq!(encoded.len(), CAND_FRAME_SIZE);
    assert_eq!(encoded[0], CAND_CMD_TX);
    assert_eq!(encoded[1], 0);
    assert_eq!(&encoded[4..8], &[0, 0, 0, 0]);
    let can_id_raw = u32::from_le_bytes([encoded[8], encoded[9], encoded[10], encoded[11]]);
    assert_eq!(can_id_raw, 0x123);
    assert_eq!(can_id_raw & CAND_ID_EFF, 0);
    assert_eq!(can_id_raw & CAND_ID_RTR, 0);
    assert_eq!(encoded[12], 4);
    assert_eq!(&encoded[16..20], &[0x01, 0x02, 0x03, 0x04]);
    assert_eq!(&encoded[20..24], &[0, 0, 0, 0]);
}

#[test]
fn test_encode_extended_frame_eff_flag() {
    let mut engine = CandleEngine::new();
    let frame = CanFrame {
        timestamp: 0,
        id: 0x12345678,
        extended: true,
        rtr: false,
        dlc: 8,
        data: vec![0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88],
        direction: CanDirection::Tx,
    };
    let encoded = engine.encode_can(&frame);
    assert_eq!(encoded.len(), CAND_FRAME_SIZE);
    let can_id_raw = u32::from_le_bytes([encoded[8], encoded[9], encoded[10], encoded[11]]);
    assert_eq!(can_id_raw & CAND_ID_MASK, 0x12345678);
    assert!(can_id_raw & CAND_ID_EFF != 0);
    assert_eq!(can_id_raw & CAND_ID_RTR, 0);
    assert_eq!(encoded[12], 8);
    assert_eq!(
        &encoded[16..24],
        &[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]
    );
}

#[test]
fn test_encode_rtr_frame_rtr_flag() {
    let mut engine = CandleEngine::new();
    let frame = CanFrame {
        timestamp: 0,
        id: 0x456,
        extended: false,
        rtr: true,
        dlc: 4,
        data: Vec::new(),
        direction: CanDirection::Tx,
    };
    let encoded = engine.encode_can(&frame);
    assert_eq!(encoded.len(), CAND_FRAME_SIZE);
    let can_id_raw = u32::from_le_bytes([encoded[8], encoded[9], encoded[10], encoded[11]]);
    assert_eq!(can_id_raw & CAND_ID_MASK, 0x456);
    assert_eq!(can_id_raw & CAND_ID_EFF, 0);
    assert!(can_id_raw & CAND_ID_RTR != 0);
    assert_eq!(encoded[12], 4);
    assert_eq!(&encoded[16..24], &[0; 8]);
}

#[test]
fn test_encode_empty_data_frame() {
    let mut engine = CandleEngine::new();
    let frame = CanFrame {
        timestamp: 0,
        id: 0x78,
        extended: false,
        rtr: false,
        dlc: 0,
        data: Vec::new(),
        direction: CanDirection::Tx,
    };
    let encoded = engine.encode_can(&frame);
    assert_eq!(encoded.len(), CAND_FRAME_SIZE);
    assert_eq!(encoded[0], CAND_CMD_TX);
    let can_id_raw = u32::from_le_bytes([encoded[8], encoded[9], encoded[10], encoded[11]]);
    assert_eq!(can_id_raw, 0x78);
    assert_eq!(encoded[12], 0);
    assert_eq!(&encoded[16..24], &[0; 8]);
}

#[test]
fn test_round_trip_standard_data_frame() {
    let mut engine = CandleEngine::new();
    let original = CanFrame {
        timestamp: 0,
        id: 0x123,
        extended: false,
        rtr: false,
        dlc: 4,
        data: vec![0x01, 0x02, 0x03, 0x04],
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

#[test]
fn test_round_trip_extended_data_frame() {
    let mut engine = CandleEngine::new();
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

#[test]
fn test_round_trip_standard_remote_frame() {
    let mut engine = CandleEngine::new();
    let original = CanFrame {
        timestamp: 0,
        id: 0x456,
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
    assert_eq!(f.data.len(), original.dlc as usize);
}

#[test]
fn test_round_trip_extended_remote_frame() {
    let mut engine = CandleEngine::new();
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
    assert_eq!(f.data.len(), original.dlc as usize);
}

#[test]
fn test_round_trip_empty_data_frame() {
    let mut engine = CandleEngine::new();
    let original = CanFrame {
        timestamp: 0,
        id: 0x78,
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
    assert_eq!(f.id, 0x78);
    assert!(!f.extended);
    assert!(!f.rtr);
    assert_eq!(f.dlc, 0);
    assert!(f.data.is_empty());
}

#[test]
fn test_round_trip_multiple_frames() {
    let mut engine = CandleEngine::new();
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
            id: 0x12345678,
            extended: true,
            rtr: false,
            dlc: 8,
            data: vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08],
            direction: CanDirection::Tx,
        },
    ];
    let mut buf = Vec::new();
    for f in &frames {
        buf.extend(engine.encode_can(f));
    }
    let parsed = engine.feed(&buf).can_frames;
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].id, 0x100);
    assert_eq!(parsed[0].data, vec![0xAA, 0xBB]);
    assert_eq!(parsed[1].id, 0x12345678);
    assert!(parsed[1].extended);
    assert_eq!(
        parsed[1].data,
        vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]
    );
}

#[test]
fn test_split_aligned_equivalence() {
    let mut data = Vec::new();
    data.extend_from_slice(&make_rx_frame(CAND_CMD_RX, 0x100, 2, &[0xAA, 0xBB]));
    data.extend_from_slice(&make_rx_frame(CAND_CMD_TX, 0x200, 1, &[0x11]));
    data.extend_from_slice(&make_rx_frame(CAND_CMD_RX, 0x300, 4, &[1, 2, 3, 4]));
    data.extend_from_slice(&make_rx_frame(CAND_CMD_RX, 0x400, 1, &[0x99])[..10]);

    let mut seq_engine = CandleEngine::new();
    let seq_frames = seq_engine.feed(&data).can_frames;

    let ranges = seq_engine
        .split_aligned(&data, 3)
        .expect("candleLight 应支持并行切分");
    let tail_start = ranges.last().map_or(0, |r| r.end);
    let mut merged = FeedOutput::default();
    let mut concat = Vec::new();
    for r in &ranges {
        let mut w = seq_engine.new_worker();
        merged.append(w.feed(&data[r.clone()]));
        concat.extend_from_slice(&data[r.clone()]);
    }
    concat.extend_from_slice(&data[tail_start..]);

    assert_eq!(concat, data);
    assert_eq!(data.len() - tail_start, 10);

    let norm = |f: &CanFrame| (f.id, f.extended, f.rtr, f.dlc, f.data.clone(), f.direction);
    let seq_norm: Vec<_> = seq_frames.iter().map(norm).collect();
    let par_norm: Vec<_> = merged.can_frames.iter().map(norm).collect();
    assert_eq!(seq_norm, par_norm);
    assert_eq!(seq_frames.len(), 3);
}
