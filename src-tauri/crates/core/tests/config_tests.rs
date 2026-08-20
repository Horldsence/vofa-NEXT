//! `core::config` 集成测试
//!
//! 覆盖:
//! - `TransportConfig` tagged enum 7 变体 serde round-trip
//! - 7 种 backend 子 config 的 Default / 字段透传 / serde
//! - `WidgetConfig` 9 种 widget + `WidgetBinding` 3 模式 + `ImageFormat`
//! - `PipelineConfig` Default / `#[serde(default)]` 字段省略 / serde round-trip

use can_types::CanBitrate;
use vofa_core::config::{
    ButtonConfig, CandleConfig, CheckboxConfig, ImageConfig, ImageFormat, KnobConfig, LabelConfig,
    PieChartConfig, PipelineConfig, RadioConfig, SerialConfig, SliderConfig, SlcanConfig,
    TcpClientConfig, TcpServerConfig, TestDataConfig, TestSignal, TransportConfig, UdpConfig,
    WaveformConfig, WidgetBinding, WidgetConfig,
};
use vofa_core::{FlowControl, Parity, StopBits};

// ============================================================
// TransportConfig enum + 7 个 backend 子 config
// ============================================================

#[test]
fn transport_config_serial_default_roundtrip() {
    let cfg = TransportConfig::Serial(SerialConfig {
        port_name: "COM3".into(),
        baud_rate: 921_600,
        data_bits: 8,
        parity: Parity::Even,
        stop_bits: StopBits::Two,
        flow_control: FlowControl::Hardware,
    });
    let json = serde_json::to_string(&cfg).unwrap();
    assert!(json.contains("\"kind\":\"Serial\""));
    let restored: TransportConfig = serde_json::from_str(&json).unwrap();
    match &restored {
        TransportConfig::Serial(s) => {
            assert_eq!(s.port_name, "COM3");
            assert_eq!(s.baud_rate, 921_600);
            assert_eq!(s.parity, Parity::Even);
            assert_eq!(s.stop_bits, StopBits::Two);
            assert_eq!(s.flow_control, FlowControl::Hardware);
        }
        _ => panic!("expected Serial variant"),
    }
}

#[test]
fn serial_config_default_matches_documented_values() {
    let s = SerialConfig::default();
    assert_eq!(s.port_name, "");
    assert_eq!(s.baud_rate, 115_200);
    assert_eq!(s.data_bits, 8);
    assert_eq!(s.parity, Parity::None);
    assert_eq!(s.stop_bits, StopBits::One);
    assert_eq!(s.flow_control, FlowControl::None);
}

#[test]
fn udp_config_default_and_roundtrip() {
    let u = UdpConfig::default();
    assert_eq!(u.local_addr, "0.0.0.0");
    assert_eq!(u.remote_addr, "127.0.0.1");
    assert_eq!(u.local_port, 0);
    assert_eq!(u.remote_port, 8888);

    let modified = UdpConfig {
        local_addr: "192.168.1.10".into(),
        remote_addr: "192.168.1.20".into(),
        local_port: 7777,
        remote_port: 9000,
    };
    let cfg = TransportConfig::Udp(modified);
    let json = serde_json::to_string(&cfg).unwrap();
    assert!(json.contains("\"kind\":\"Udp\""));
    let restored: TransportConfig = serde_json::from_str(&json).unwrap();
    match restored {
        TransportConfig::Udp(u) => {
            assert_eq!(u.local_port, 7777);
            assert_eq!(u.remote_port, 9000);
        }
        _ => panic!("expected Udp variant"),
    }
}

#[test]
fn tcp_client_default_and_roundtrip() {
    let c = TcpClientConfig::default();
    assert_eq!(c.host, "127.0.0.1");
    assert_eq!(c.port, 8888);

    let cfg = TransportConfig::TcpClient(TcpClientConfig {
        host: "10.0.0.1".into(),
        port: 502,
    });
    let restored: TransportConfig = serde_json::from_str(&serde_json::to_string(&cfg).unwrap()).unwrap();
    match restored {
        TransportConfig::TcpClient(c) => {
            assert_eq!(c.host, "10.0.0.1");
            assert_eq!(c.port, 502);
        }
        _ => panic!("expected TcpClient variant"),
    }
}

#[test]
fn tcp_server_default_and_roundtrip() {
    let s = TcpServerConfig::default();
    assert_eq!(s.listen_addr, "0.0.0.0");
    assert_eq!(s.listen_port, 8888);

    let cfg = TransportConfig::TcpServer(TcpServerConfig {
        listen_addr: "::".into(),
        listen_port: 7777,
    });
    let restored: TransportConfig = serde_json::from_str(&serde_json::to_string(&cfg).unwrap()).unwrap();
    match restored {
        TransportConfig::TcpServer(s) => {
            assert_eq!(s.listen_addr, "::");
            assert_eq!(s.listen_port, 7777);
        }
        _ => panic!("expected TcpServer variant"),
    }
}

#[test]
fn test_data_config_default_and_signal_serde() {
    let td = TestDataConfig::default();
    assert_eq!(td.channels, 4);
    assert_eq!(td.sample_rate, 1000.0);
    assert_eq!(td.signal, TestSignal::Sine);

    // 所有 10 个 TestSignal 变体 round-trip
    for sig in [
        TestSignal::Sine,
        TestSignal::Square,
        TestSignal::Triangle,
        TestSignal::Sawtooth,
        TestSignal::Random,
        TestSignal::Dc,
        TestSignal::Chirp,
        TestSignal::Steps,
        TestSignal::Noise,
        TestSignal::MultiTone,
    ] {
        let json = serde_json::to_string(&sig).unwrap();
        let restored: TestSignal = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, sig);
    }
}

#[test]
fn slcan_config_default_and_roundtrip() {
    let s = SlcanConfig::default();
    assert_eq!(s.port_name, "");
    assert_eq!(s.baud_rate, 115_200);
    assert_eq!(s.can_bitrate, CanBitrate::Bps500k);

    let cfg = TransportConfig::Slcan(SlcanConfig {
        port_name: "/dev/cu.usbserial-1410".into(),
        baud_rate: 1_000_000,
        can_bitrate: CanBitrate::Bps1m,
    });
    let restored: TransportConfig = serde_json::from_str(&serde_json::to_string(&cfg).unwrap()).unwrap();
    match restored {
        TransportConfig::Slcan(s) => {
            assert_eq!(s.port_name, "/dev/cu.usbserial-1410");
            assert_eq!(s.baud_rate, 1_000_000);
            assert_eq!(s.can_bitrate, CanBitrate::Bps1m);
        }
        _ => panic!("expected Slcan variant"),
    }
}

#[test]
fn candle_config_default_and_roundtrip() {
    let c = CandleConfig::default();
    assert_eq!(c.bus, 0);
    assert_eq!(c.address, 0);
    assert_eq!(c.can_bitrate, CanBitrate::Bps500k);
    assert_eq!(c.channel, 0);

    let cfg = TransportConfig::CandleLight(CandleConfig {
        bus: 1,
        address: 0xDEAD_BEEF_u32 as u8,
        can_bitrate: CanBitrate::Bps250k,
        channel: 1,
    });
    let restored: TransportConfig = serde_json::from_str(&serde_json::to_string(&cfg).unwrap()).unwrap();
    match restored {
        TransportConfig::CandleLight(c) => {
            assert_eq!(c.bus, 1);
            assert_eq!(c.can_bitrate, CanBitrate::Bps250k);
            assert_eq!(c.channel, 1);
        }
        _ => panic!("expected CandleLight variant"),
    }
}

// ============================================================
// WidgetConfig 9 种控件 + ImageFormat + WidgetBinding
// ============================================================

#[test]
fn knob_widget_roundtrip_with_auto_binding() {
    let w = WidgetConfig::Knob(KnobConfig {
        id: "k1".into(),
        label: "Volume".into(),
        min: 0.0,
        max: 100.0,
        step: 1.0,
        default: 50.0,
        binding: WidgetBinding::Auto { channel: 3 },
    });
    let json = serde_json::to_string(&w).unwrap();
    assert!(json.contains("\"kind\":\"Knob\""));
    let restored: WidgetConfig = serde_json::from_str(&json).unwrap();
    match restored {
        WidgetConfig::Knob(k) => {
            assert_eq!(k.id, "k1");
            assert_eq!(k.default, 50.0);
            assert!(matches!(k.binding, WidgetBinding::Auto { channel: 3 }));
        }
        _ => panic!("expected Knob variant"),
    }
}

#[test]
fn button_widget_with_manual_template_binding() {
    let w = WidgetConfig::Button(ButtonConfig {
        id: "b1".into(),
        label: "Send".into(),
        press_value: 1.0,
        release_value: 0.0,
        binding: WidgetBinding::Manual {
            template: "AT+CMD={value}\r\n".into(),
        },
    });
    let json = serde_json::to_string(&w).unwrap();
    assert!(json.contains("\"mode\":\"Manual\""));
    assert!(json.contains("{value}"));
    let restored: WidgetConfig = serde_json::from_str(&json).unwrap();
    match restored {
        WidgetConfig::Button(b) => {
            assert_eq!(b.press_value, 1.0);
            assert_eq!(b.release_value, 0.0);
        }
        _ => panic!("expected Button variant"),
    }
}

#[test]
fn radio_widget_options_roundtrip() {
    let w = WidgetConfig::Radio(RadioConfig {
        id: "r1".into(),
        label: "Mode".into(),
        options: vec![("Low".into(), 1.0), ("Mid".into(), 2.0), ("High".into(), 3.0)],
        default: 1,
        binding: WidgetBinding::None,
    });
    let json = serde_json::to_string(&w).unwrap();
    let restored: WidgetConfig = serde_json::from_str(&json).unwrap();
    match restored {
        WidgetConfig::Radio(r) => {
            assert_eq!(r.options.len(), 3);
            assert_eq!(r.options[2], ("High".into(), 3.0));
            assert_eq!(r.default, 1);
            assert!(matches!(r.binding, WidgetBinding::None));
        }
        _ => panic!("expected Radio variant"),
    }
}

#[test]
fn checkbox_and_slider_widget_roundtrip() {
    let cb = WidgetConfig::Checkbox(CheckboxConfig {
        id: "c1".into(),
        label: "Enable".into(),
        checked_value: 1.0,
        unchecked_value: 0.0,
        default: true,
        binding: WidgetBinding::Auto { channel: 0 },
    });
    let json = serde_json::to_string(&cb).unwrap();
    let restored: WidgetConfig = serde_json::from_str(&json).unwrap();
    match restored {
        WidgetConfig::Checkbox(c) => {
            assert!(c.default);
            assert_eq!(c.checked_value, 1.0);
        }
        _ => panic!("expected Checkbox variant"),
    }

    let sl = WidgetConfig::Slider(SliderConfig {
        id: "s1".into(),
        label: "Speed".into(),
        min: 0.0,
        max: 200.0,
        step: 0.5,
        default: 100.0,
        binding: WidgetBinding::None,
    });
    let json = serde_json::to_string(&sl).unwrap();
    let restored: WidgetConfig = serde_json::from_str(&json).unwrap();
    match restored {
        WidgetConfig::Slider(s) => {
            assert_eq!(s.max, 200.0);
            assert_eq!(s.step, 0.5);
        }
        _ => panic!("expected Slider variant"),
    }
}

#[test]
fn label_widget_with_optional_channel() {
    let w = WidgetConfig::Label(LabelConfig {
        id: "l1".into(),
        text: "Hello".into(),
        channel: Some(7),
    });
    let json = serde_json::to_string(&w).unwrap();
    let restored: WidgetConfig = serde_json::from_str(&json).unwrap();
    match restored {
        WidgetConfig::Label(l) => {
            assert_eq!(l.text, "Hello");
            assert_eq!(l.channel, Some(7));
        }
        _ => panic!("expected Label variant"),
    }

    // None 通道应正常 round-trip
    let w = WidgetConfig::Label(LabelConfig {
        id: "l2".into(),
        text: "Static".into(),
        channel: None,
    });
    let json = serde_json::to_string(&w).unwrap();
    assert!(json.contains("\"channel\":null"));
}

#[test]
fn waveform_widget_visible_channels_vec_roundtrip() {
    let w = WidgetConfig::Waveform(WaveformConfig {
        id: "wf1".into(),
        channels: 4,
        max_points: 1024,
        visible_channels: vec![true, false, true, false],
    });
    let json = serde_json::to_string(&w).unwrap();
    let restored: WidgetConfig = serde_json::from_str(&json).unwrap();
    match restored {
        WidgetConfig::Waveform(w) => {
            assert_eq!(w.channels, 4);
            assert_eq!(w.max_points, 1024);
            assert_eq!(w.visible_channels, vec![true, false, true, false]);
        }
        _ => panic!("expected Waveform variant"),
    }
}

#[test]
fn pie_chart_widget_segments_and_channels() {
    let w = WidgetConfig::PieChart(PieChartConfig {
        id: "p1".into(),
        label: "Share".into(),
        segments: vec!["A".into(), "B".into(), "C".into()],
        channels: vec![0, 1, 2],
    });
    let json = serde_json::to_string(&w).unwrap();
    let restored: WidgetConfig = serde_json::from_str(&json).unwrap();
    match restored {
        WidgetConfig::PieChart(p) => {
            assert_eq!(p.segments.len(), 3);
            assert_eq!(p.channels, vec![0, 1, 2]);
        }
        _ => panic!("expected PieChart variant"),
    }
}

#[test]
fn image_widget_format_uses_lowercase_serde() {
    for fmt in [ImageFormat::Rgb888, ImageFormat::Rgb565, ImageFormat::Gray8] {
        let json = serde_json::to_string(&fmt).unwrap();
        // rename_all = "lowercase"
        assert!(json.contains("rgb888") || json.contains("rgb565") || json.contains("gray8"));
        let restored: ImageFormat = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, fmt);
    }

    let w = WidgetConfig::Image(ImageConfig {
        id: "img1".into(),
        label: "Preview".into(),
        width: 320,
        height: 240,
        format: ImageFormat::Rgb565,
    });
    let json = serde_json::to_string(&w).unwrap();
    let restored: WidgetConfig = serde_json::from_str(&json).unwrap();
    match restored {
        WidgetConfig::Image(i) => {
            assert_eq!(i.width, 320);
            assert_eq!(i.height, 240);
            assert_eq!(i.format, ImageFormat::Rgb565);
        }
        _ => panic!("expected Image variant"),
    }
}

// ============================================================
// PipelineConfig — #[serde(default)] + round-trip
// ============================================================

#[test]
fn pipeline_config_default_matches_documented_values() {
    let p = PipelineConfig::default();
    assert_eq!(p.coalesce_max_msgs, 64);
    assert_eq!(p.coalesce_max_bytes_kb, 256);
    assert_eq!(p.max_feed_workers, 4);
    assert_eq!(p.feed_parallel_unit, 8);
    assert_eq!(p.min_worker_bytes_kb, 32);
    assert_eq!(p.max_stream_shards, 4);
    assert_eq!(p.parse_channel_cap, 256);
}

#[test]
fn pipeline_config_partial_json_fills_missing_with_defaults() {
    // 只给 coalesce_max_msgs,其他字段应回退到默认值
    let json = r#"{"coalesce_max_msgs":128}"#;
    let p: PipelineConfig = serde_json::from_str(json).unwrap();
    assert_eq!(p.coalesce_max_msgs, 128);
    assert_eq!(p.coalesce_max_bytes_kb, 256);
    assert_eq!(p.max_feed_workers, 4);
    assert_eq!(p.parse_channel_cap, 256);
}

#[test]
fn pipeline_config_empty_object_uses_full_default() {
    let p: PipelineConfig = serde_json::from_str("{}").unwrap();
    assert_eq!(p, PipelineConfig::default());
}

#[test]
fn pipeline_config_full_roundtrip() {
    let p = PipelineConfig {
        coalesce_max_msgs: 32,
        coalesce_max_bytes_kb: 128,
        max_feed_workers: 8,
        feed_parallel_unit: 4,
        min_worker_bytes_kb: 64,
        max_stream_shards: 2,
        parse_channel_cap: 512,
    };
    let json = serde_json::to_string(&p).unwrap();
    let restored: PipelineConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, p);
}